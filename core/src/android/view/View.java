package android.view;

import android.content.Context;
import android.content.res.Resources;
import android.graphics.drawable.ColorDrawable;
import android.graphics.drawable.Drawable;
import android.os.Handler;
import android.util.AttributeSet;

public class View implements Drawable.Callback {
    public static final int VISIBLE = 0, INVISIBLE = 4, GONE = 8, NO_ID = -1;
    public static final int TEXT_ALIGNMENT_CENTER = 4, TEXT_ALIGNMENT_VIEW_START = 5, TEXT_ALIGNMENT_GRAVITY = 1;
    public interface OnClickListener { void onClick(View v); }
    public interface OnLongClickListener { boolean onLongClick(View v); }
    public interface OnTouchListener { boolean onTouch(View v, MotionEvent e); }
    public interface OnFocusChangeListener { void onFocusChange(View v, boolean hasFocus); }
    public interface OnKeyListener { boolean onKey(View v, int keyCode, KeyEvent e); }
    public interface OnLayoutChangeListener { void onLayoutChange(View v, int l, int t, int r, int b, int ol, int ot, int or, int ob); }
    public interface OnAttachStateChangeListener { void onViewAttachedToWindow(View v); void onViewDetachedFromWindow(View v); }

    private static int nextUid = 1;
    private static final Handler HANDLER = new Handler();
    /** Host-visible identity (stable for the life of the view). */
    public final int indroidUid = nextUid++;
    private final Context mContext;
    private int mId = NO_ID;
    ViewParent mParent;
    private ViewGroup.LayoutParams mLayoutParams;
    private int visibility = VISIBLE;
    private boolean enabled = true, clickable, selected, focusable;
    int pl, pt, pr, pb;
    private Drawable mBackground, mForeground;
    private android.content.res.ColorStateList bgTint;
    private boolean pressed, activated, hovered;
    private float alpha = 1f;
    private int minW, minH;
    private Object tag;
    private android.util.SparseArray<Object> keyedTags;
    private CharSequence contentDescription;
    protected OnClickListener mOnClickListener;
    private OnLongClickListener longClick;
    private OnTouchListener touchListener;
    private static boolean dirty;
    private OnFocusChangeListener focusListener;

    public View(Context context) { mContext = context; }
    public View(Context context, AttributeSet attrs) { this(context, attrs, 0); }
    public View(Context context, AttributeSet attrs, int defStyle) { this(context, attrs, defStyle, 0); }
    public View(Context context, AttributeSet attrs, int defStyleAttr, int defStyleRes) {
        mContext = context;
        if (context != null && (attrs != null || defStyleAttr != 0 || defStyleRes != 0)) applyStyled(context.obtainStyledAttributes(attrs, VIEW_ATTRS, defStyleAttr, defStyleRes));
    }

    // android.R.attr ids read by View (sorted is not required: TypedArray indexes by position)
    static final int[] VIEW_ATTRS = { 16842960 /*id*/, 16842964 /*background*/, 16842965 /*padding*/, 16842966 /*paddingLeft*/,
        16842967 /*paddingTop*/, 16842968 /*paddingRight*/, 16842969 /*paddingBottom*/, 16843699 /*paddingStart*/, 16843700 /*paddingEnd*/,
        16844093 /*paddingHorizontal*/, 16844094 /*paddingVertical*/, 16842972 /*visibility*/, 16842981 /*clickable*/, 16842982 /*longClickable*/,
        16842766 /*enabled*/, 16842970 /*focusable*/, 16843071 /*minWidth*/, 16843072 /*minHeight*/, 16843551 /*alpha*/,
        16843379 /*contentDescription*/, 16843840 /*elevation*/, 16843017 /*foreground*/, 16843375 /*onClick*/, 16842961 /*tag*/,
        16843883 /*backgroundTint*/, 16843770 /*translationZ*/ };

    private void applyStyled(android.content.res.TypedArray a) {
        mId = a.getResourceId(0, NO_ID);
        if (a.hasValue(1)) { try { Drawable d = a.getDrawable(1); if (d != null) { mBackground = d; d.setCallback(this); } } catch (RuntimeException e) {} }
        int p = a.getDimensionPixelSize(2, -1);
        if (p >= 0) pl = pt = pr = pb = p;
        int ph = a.getDimensionPixelSize(9, -1), pv = a.getDimensionPixelSize(10, -1);
        if (ph >= 0) pl = pr = ph;
        if (pv >= 0) pt = pb = pv;
        pl = a.getDimensionPixelSize(3, a.getDimensionPixelSize(7, pl));
        pr = a.getDimensionPixelSize(5, a.getDimensionPixelSize(8, pr));
        pt = a.getDimensionPixelSize(4, pt);
        pb = a.getDimensionPixelSize(6, pb);
        if (a.hasValue(11)) {
            String vs = a.getNonResourceString(11);
            int v = vs != null ? (vs.equals("gone") ? 2 : vs.equals("invisible") ? 1 : 0) : a.getInt(11, 0);
            visibility = v == 2 ? GONE : v == 1 ? INVISIBLE : VISIBLE;
        }
        clickable = a.getBoolean(12, clickable);
        longClickable = a.getBoolean(13, false);
        enabled = a.getBoolean(14, true);
        focusable = a.getBoolean(15, focusable);
        minW = a.getDimensionPixelSize(16, minW);
        minH = a.getDimensionPixelSize(17, minH);
        alpha = a.getFloat(18, 1f);
        CharSequence cd = a.getText(19);
        if (cd != null) contentDescription = cd;
        if (a.hasValue(20)) setElevation(a.getDimension(20, 0));
        if (a.hasValue(21)) { try { mForeground = a.getDrawable(21); if (mForeground != null) mForeground.setCallback(this); } catch (RuntimeException e) {} }
        String onClick = a.getString(22);
        if (onClick != null) {
            final String method = onClick;
            mOnClickListener = new OnClickListener() {
                public void onClick(View v) { VMBridge.invokeOnClick(getContext(), method, v); }
            };
        }
        CharSequence tg = a.getText(23);
        if (tg != null) tag = tg;
        if (a.hasValue(24)) { android.content.res.ColorStateList t = a.getColorStateList(24); if (t != null) setBackgroundTintList(t); }
        a.recycle();
    }

    public Context getContext() { return mContext; }
    public Resources getResources() { return mContext.getResources(); }
    public int getId() { return mId; }
    public void setId(int id) { mId = id; }
    public static int generateViewId() { return 0x00ff0000 + nextUid++; }
    @SuppressWarnings("unchecked")
    public final <T extends View> T findViewById(int id) { return id == NO_ID ? null : (T) findViewTraversal(id); }
    @SuppressWarnings("unchecked")
    public final <T extends View> T requireViewById(int id) {
        T v = findViewById(id);
        if (v == null) throw new IllegalArgumentException("ID does not reference a View inside this View");
        return v;
    }
    @SuppressWarnings("unchecked")
    public final <T extends View> T findViewWithTag(Object t) { return (T) findWithTag(t); }
    View findWithTag(Object t) { return t != null && t.equals(tag) ? this : null; }
    View findViewTraversal(int id) { return id == mId ? this : null; }
    View findByUid(int uid) { return uid == indroidUid ? this : null; }
    public final ViewParent getParent() { return mParent; }
    public View getRootView() { View v = this; while (v.mParent instanceof View) v = (View) v.mParent; return v; }
    public ViewGroup.LayoutParams getLayoutParams() { return mLayoutParams; }
    public void setLayoutParams(ViewGroup.LayoutParams p) { if (p == null) throw new NullPointerException("Layout parameters cannot be null"); mLayoutParams = p; p.resolveLayoutDirection(getLayoutDirection()); requestLayout(); }

    public void setOnClickListener(OnClickListener l) { if (l != null) clickable = true; mOnClickListener = l; }
    public boolean hasOnClickListeners() { return mOnClickListener != null; }
    public void setOnLongClickListener(OnLongClickListener l) { longClick = l; }
    public void setOnTouchListener(OnTouchListener l) { touchListener = l; }
    public boolean dispatchTouchEvent(MotionEvent e) { return enabled && ((touchListener != null && touchListener.onTouch(this,e)) || onTouchEvent(e)); }
    public boolean onTouchEvent(MotionEvent e) {
        boolean clickableNow = clickable || mOnClickListener != null || isLongClickable();
        if (!enabled) return clickableNow;
        if (!clickableNow) return false;
        switch (e.getActionMasked()) {
            case MotionEvent.ACTION_DOWN: setPressed(true); break;
            case MotionEvent.ACTION_MOVE:
                if (e.getX() < -8 || e.getY() < -8 || e.getX() > getWidth() + 8 || e.getY() > getHeight() + 8) setPressed(false);
                break;
            case MotionEvent.ACTION_UP:
                if (pressed) { setPressed(false); performClick(); }
                break;
            case MotionEvent.ACTION_CANCEL: setPressed(false); break;
        }
        return true;
    }
    public void setOnFocusChangeListener(OnFocusChangeListener l) { focusListener = l; }
    public OnFocusChangeListener getOnFocusChangeListener() { return focusListener; }
    public void setOnKeyListener(OnKeyListener l) {}
    private java.util.ArrayList<OnAttachStateChangeListener> attachListeners;
    boolean mAttached;
    public void addOnAttachStateChangeListener(OnAttachStateChangeListener l) { if (attachListeners == null) attachListeners = new java.util.ArrayList<OnAttachStateChangeListener>(); attachListeners.add(l); }
    public void removeOnAttachStateChangeListener(OnAttachStateChangeListener l) { if (attachListeners != null) attachListeners.remove(l); }
    void indroidAttached() { if (mParent instanceof View && ((View) mParent).mAttached) attachTree(true); }
    void indroidDetached() { if (mAttached) attachTree(false); }
    /** indroid: attach/detach this subtree to the (single) window. */
    public void indroidSetAttached(boolean a) { if (a != mAttached) attachTree(a); }
    void attachTree(boolean attach) {
        if (attach == mAttached) return;
        mAttached = attach;
        if (attach) {
            if (mLayoutParams != null) mLayoutParams.resolveLayoutDirection(getLayoutDirection());
            onAttachedToWindow();
            if (attachListeners != null) for (OnAttachStateChangeListener l : new java.util.ArrayList<OnAttachStateChangeListener>(attachListeners)) l.onViewAttachedToWindow(this);
        } else {
            onDetachedFromWindow();
            if (attachListeners != null) for (OnAttachStateChangeListener l : new java.util.ArrayList<OnAttachStateChangeListener>(attachListeners)) l.onViewDetachedFromWindow(this);
        }
    }
    protected void onAttachedToWindow() {}
    protected void onDetachedFromWindow() {}
    public boolean performClick() {
        if (mOnClickListener != null) { mOnClickListener.onClick(this); return true; }
        return false;
    }
    public boolean callOnClick() { return performClick(); }
    public boolean performLongClick() { return longClick != null && longClick.onLongClick(this); }
    public void setVisibility(int v) { if (v != visibility) { boolean gone = v == GONE || visibility == GONE; visibility = v; if (gone) requestLayout(); } }
    public int getVisibility() { return visibility; }
    public boolean isShown() { return visibility == VISIBLE; }
    public void setEnabled(boolean e) { if (e != enabled) { enabled = e; refreshDrawableState(); invalidate(); } }
    public boolean isEnabled() { return enabled; }
    public void setClickable(boolean c) { clickable = c; }
    public boolean isClickable() { return clickable; }
    public void setSelected(boolean s) { if (s != selected) { selected = s; refreshDrawableState(); } }
    public boolean isSelected() { return selected; }
    public void setFocusable(boolean f) { focusable = f; }
    public void setFocusableInTouchMode(boolean f) {}
    public boolean isFocusable() { return focusable; }
    public boolean requestFocus() { return true; }
    public void clearFocus() {}
    public boolean hasFocus() { return false; }
    public boolean isFocused() { return false; }
    public void setPadding(int l, int t, int r, int b) { if (l != pl || t != pt || r != pr || b != pb) { pl = l; pt = t; pr = r; pb = b; requestLayout(); } }
    public void setPaddingRelative(int s, int t, int e, int b) { setPadding(s, t, e, b); }
    public int getPaddingLeft() { return pl; }
    public int getPaddingTop() { return pt; }
    public int getPaddingRight() { return pr; }
    public int getPaddingBottom() { return pb; }
    public int getPaddingStart() { return pl; }
    public int getPaddingEnd() { return pr; }
    public void setBackgroundColor(int c) {
        if (mBackground instanceof ColorDrawable) { ((ColorDrawable) mBackground.mutate()).setColor(c); invalidate(); }
        else setBackground(new ColorDrawable(c));
    }
    public void setBackgroundResource(int id) { setBackground(id == 0 ? null : getContext().getDrawable(id)); }
    public void setBackground(Drawable d) { setBackgroundDrawable(d); }
    @Deprecated
    public void setBackgroundDrawable(Drawable d) {
        if (d == mBackground) return;
        if (mBackground != null) mBackground.setCallback(null);
        boolean sizeChange = d == null || mBackground == null || d.getMinimumWidth() != mBackground.getMinimumWidth() || d.getMinimumHeight() != mBackground.getMinimumHeight();
        mBackground = d;
        if (d != null) {
            d.setCallback(this);
            if (d.isStateful()) d.setState(getDrawableState());
            if (bgTint != null) d.setTintList(bgTint);
        }
        if (sizeChange) requestLayout();
        invalidate();
    }
    public Drawable getBackground() { return mBackground; }
    public void setBackgroundTintList(android.content.res.ColorStateList t) { bgTint = t; if (mBackground != null) { mBackground = mBackground.mutate(); mBackground.setTintList(t); } invalidate(); }
    public android.content.res.ColorStateList getBackgroundTintList() { return bgTint; }
    public void setForeground(Drawable d) { if (mForeground != null) mForeground.setCallback(null); mForeground = d; if (d != null) d.setCallback(this); invalidate(); }
    public Drawable getForeground() { return mForeground; }

    // ---------------------------------------------------------------- drawables & state
    public void invalidateDrawable(Drawable who) { invalidate(); }
    public void scheduleDrawable(Drawable who, Runnable what, long when) { HANDLER.postAtTime(what, who, when); }
    public void unscheduleDrawable(Drawable who, Runnable what) { HANDLER.removeCallbacks(what, who); }
    public void unscheduleDrawable(Drawable who) { HANDLER.removeCallbacksAndMessages(who); }
    protected boolean verifyDrawable(Drawable who) { return who == mBackground || who == mForeground; }
    public static final int[] EMPTY_STATE_SET = new int[0];
    public static final int[] ENABLED_STATE_SET = { 0x0101009e }, PRESSED_ENABLED_STATE_SET = { 0x010100a7, 0x0101009e }, SELECTED_STATE_SET = { 0x010100a1 },
        FOCUSED_STATE_SET = { 0x0101009c }, PRESSED_STATE_SET = { 0x010100a7 }, ENABLED_FOCUSED_STATE_SET = { 0x0101009e, 0x0101009c }, ENABLED_SELECTED_STATE_SET = { 0x0101009e, 0x010100a1 };
    protected int[] onCreateDrawableState(int extraSpace) {
        int n = 0;
        int[] tmp = new int[8 + extraSpace];
        if (enabled) tmp[n++] = 0x0101009e;
        if (pressed) tmp[n++] = 0x010100a7;
        if (selected) tmp[n++] = 0x010100a1;
        if (activated) tmp[n++] = 0x010102fe;
        if (hovered) tmp[n++] = 0x01010367;
        tmp[n++] = 0x0101009d; // window focused
        int[] out = new int[n + extraSpace];
        System.arraycopy(tmp, 0, out, 0, n);
        return out;
    }
    protected static int[] mergeDrawableStates(int[] base, int[] additional) {
        int i = base.length - 1;
        while (i >= 0 && base[i] == 0) i--;
        System.arraycopy(additional, 0, base, i + 1, additional.length);
        return base;
    }
    public final int[] getDrawableState() { return onCreateDrawableState(0); }
    public void refreshDrawableState() { drawableStateChanged(); if (mParent instanceof ViewGroup) ((ViewGroup) mParent).childDrawableStateChanged(this); }
    protected void drawableStateChanged() {
        int[] st = getDrawableState();
        boolean changed = false;
        if (mBackground != null && mBackground.isStateful()) changed |= mBackground.setState(st);
        if (mForeground != null && mForeground.isStateful()) changed |= mForeground.setState(st);
        if (changed) invalidate();
    }
    public void jumpDrawablesToCurrentState() { if (mBackground != null) mBackground.jumpToCurrentState(); if (mForeground != null) mForeground.jumpToCurrentState(); }
    public void drawableHotspotChanged(float x, float y) {}
    public void setPressed(boolean p) { if (p != pressed) { pressed = p; refreshDrawableState(); } }
    public boolean isPressed() { return pressed; }
    public void setActivated(boolean a) { if (a != activated) { activated = a; refreshDrawableState(); } }
    public boolean isActivated() { return activated; }
    public void setHovered(boolean h) { if (h != hovered) { hovered = h; refreshDrawableState(); } }
    public boolean isHovered() { return hovered; }
    /** Android's draw order: background, content, children (host), foreground. */
    public void draw(android.graphics.Canvas c) {
        if (mBackground != null) { mBackground.setBounds(0, 0, getWidth(), getHeight()); mBackground.draw(c); }
        onDraw(c);
        dispatchDraw(c);
        onDrawForeground(c);
    }
    public void onDrawForeground(android.graphics.Canvas c) {
        if (mForeground != null) { mForeground.setBounds(0, 0, getWidth(), getHeight()); mForeground.draw(c); }
    }
    protected void dispatchDraw(android.graphics.Canvas c) {}
    /** indroid: whether this node needs canvas ops (custom drawing or a non-color background). */
    protected boolean indroidNeedsCanvas() {
        return (mBackground != null && !(mBackground instanceof ColorDrawable)) || mForeground != null || VMViews.overridesOnDraw(this);
    }
    public void setAlpha(float a) { alpha = a; }
    public float getAlpha() { return alpha; }
    public void setMinimumWidth(int w) { minW = w; requestLayout(); }
    public void setMinimumHeight(int h) { minH = h; requestLayout(); }
    public void setTag(Object t) { tag = t; }
    public Object getTag() { return tag; }
    public void setTag(int key, Object t) { if (keyedTags == null) keyedTags = new android.util.SparseArray<Object>(); keyedTags.put(key, t); }
    public Object getTag(int key) { return keyedTags == null ? null : keyedTags.get(key); }
    public void setContentDescription(CharSequence c) { contentDescription = c; }
    public CharSequence getContentDescription() { return contentDescription; }
    public boolean post(Runnable r) { return HANDLER.post(r); }
    public boolean postDelayed(Runnable r, long ms) { return HANDLER.postDelayed(r, ms); }
    public boolean removeCallbacks(Runnable r) { HANDLER.removeCallbacks(r); return true; }
    public Handler getHandler() { return HANDLER; }
    public void invalidate() { if (mAttached && indroidNeedsCanvas()) dirty = true; }
    public void postInvalidate() { invalidate(); }
    public void postInvalidateOnAnimation() { invalidate(); }
    public void postOnAnimation(Runnable r) { postDelayed(r,16); }
    public static boolean indroidTakeDirty() { boolean d=dirty; dirty=false; return d; }
    protected void onSizeChanged(int w,int h,int oldw,int oldh) {}
    protected void onDraw(android.graphics.Canvas c) {}
    /** indroid: host-reported size (legacy path; layout normally sets the frame). */
    public void indroidSetSize(int w,int h) { if (w != getWidth() || h != getHeight()) setFrame(mLeft, mTop, mLeft + w, mTop + h); }

    // ---------------------------------------------------------------- measure/layout
    public static class MeasureSpec {
        private static final int MODE_SHIFT = 30, MODE_MASK = 3 << MODE_SHIFT;
        public static final int UNSPECIFIED = 0, EXACTLY = 1 << MODE_SHIFT, AT_MOST = 2 << MODE_SHIFT;
        public static int makeMeasureSpec(int size, int mode) { return (size & ~MODE_MASK) | (mode & MODE_MASK); }
        public static int makeSafeMeasureSpec(int size, int mode) { return makeMeasureSpec(size, mode); }
        public static int getMode(int spec) { return spec & MODE_MASK; }
        public static int getSize(int spec) { return spec & ~MODE_MASK; }
        public static String toString(int spec) {
            int m = getMode(spec);
            return "MeasureSpec: " + (m == EXACTLY ? "EXACTLY " : m == AT_MOST ? "AT_MOST " : "UNSPECIFIED ") + getSize(spec);
        }
    }
    public static final int MEASURED_SIZE_MASK = 0x00ffffff, MEASURED_STATE_MASK = 0xff000000, MEASURED_HEIGHT_STATE_SHIFT = 16, MEASURED_STATE_TOO_SMALL = 0x01000000;

    int mLeft, mTop, mRight, mBottom;
    private int mMeasuredWidth, mMeasuredHeight;
    private int mOldWidthSpec = Integer.MIN_VALUE, mOldHeightSpec = Integer.MIN_VALUE;
    boolean mForceLayout = true, mLayoutRequired = true;
    private boolean measuredCalled;
    int mScrollX, mScrollY;
    private float translationX, translationY, scaleX = 1, scaleY = 1, rotation;
    private java.util.ArrayList<OnLayoutChangeListener> layoutListeners;
    private static boolean sLayoutRequested = true;
    /** indroid: true once since the last call when any view asked for layout. */
    public static boolean indroidTakeLayoutRequest() { boolean r = sLayoutRequested; sLayoutRequested = false; return r; }

    public final void measure(int widthMeasureSpec, int heightMeasureSpec) {
        if (mForceLayout || widthMeasureSpec != mOldWidthSpec || heightMeasureSpec != mOldHeightSpec) {
            if (mLayoutParams != null) mLayoutParams.resolveLayoutDirection(getLayoutDirection());
            measuredCalled = false;
            onMeasure(widthMeasureSpec, heightMeasureSpec);
            if (!measuredCalled) throw new IllegalStateException("View with id " + getId() + ": " + getClass().getName() + "#onMeasure() did not set the measured dimension by calling setMeasuredDimension()");
            mOldWidthSpec = widthMeasureSpec;
            mOldHeightSpec = heightMeasureSpec;
            mLayoutRequired = true;
        }
    }
    protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
        setMeasuredDimension(getDefaultSize(getSuggestedMinimumWidth(), widthMeasureSpec), getDefaultSize(getSuggestedMinimumHeight(), heightMeasureSpec));
    }
    protected final void setMeasuredDimension(int w, int h) { mMeasuredWidth = w; mMeasuredHeight = h; measuredCalled = true; }
    public static int getDefaultSize(int size, int spec) {
        int mode = MeasureSpec.getMode(spec), s = MeasureSpec.getSize(spec);
        return mode == MeasureSpec.UNSPECIFIED ? size : s;
    }
    public static int resolveSize(int size, int spec) { return resolveSizeAndState(size, spec, 0) & MEASURED_SIZE_MASK; }
    public static int resolveSizeAndState(int size, int spec, int childState) {
        int mode = MeasureSpec.getMode(spec), s = MeasureSpec.getSize(spec);
        int result;
        if (mode == MeasureSpec.AT_MOST) result = s < size ? (s | MEASURED_STATE_TOO_SMALL) : size;
        else if (mode == MeasureSpec.EXACTLY) result = s;
        else result = size;
        return result | (childState & MEASURED_STATE_MASK);
    }
    public static int combineMeasuredStates(int a, int b) { return a | b; }
    protected int getSuggestedMinimumWidth() { return mBackground == null ? minW : Math.max(minW, mBackground.getMinimumWidth()); }
    protected int getSuggestedMinimumHeight() { return mBackground == null ? minH : Math.max(minH, mBackground.getMinimumHeight()); }
    public int getMinimumWidth() { return minW; }
    public int getMinimumHeight() { return minH; }
    public final int getMeasuredWidth() { return mMeasuredWidth & MEASURED_SIZE_MASK; }
    public final int getMeasuredHeight() { return mMeasuredHeight & MEASURED_SIZE_MASK; }
    public final int getMeasuredWidthAndState() { return mMeasuredWidth; }
    public final int getMeasuredHeightAndState() { return mMeasuredHeight; }
    public final int getMeasuredState() { return (mMeasuredWidth & MEASURED_STATE_MASK) | ((mMeasuredHeight >> MEASURED_HEIGHT_STATE_SHIFT) & (MEASURED_STATE_MASK >> MEASURED_HEIGHT_STATE_SHIFT)); }

    public void layout(int l, int t, int r, int b) {
        int ol = mLeft, ot = mTop, or = mRight, ob = mBottom;
        boolean changed = setFrame(l, t, r, b);
        if (changed || mLayoutRequired) {
            onLayout(changed, l, t, r, b);
            mLayoutRequired = false;
            if (layoutListeners != null) for (OnLayoutChangeListener li : new java.util.ArrayList<OnLayoutChangeListener>(layoutListeners)) li.onLayoutChange(this, l, t, r, b, ol, ot, or, ob);
        }
        mForceLayout = false;
    }
    protected void onLayout(boolean changed, int l, int t, int r, int b) {}
    protected boolean setFrame(int l, int t, int r, int b) {
        if (mLeft == l && mTop == t && mRight == r && mBottom == b) return false;
        int ow = mRight - mLeft, oh = mBottom - mTop, nw = r - l, nh = b - t;
        mLeft = l; mTop = t; mRight = r; mBottom = b;
        if (nw != ow || nh != oh) { onSizeChanged(nw, nh, ow, oh); invalidate(); }
        return true;
    }
    public void requestLayout() {
        mForceLayout = true;
        mLayoutRequired = true;
        sLayoutRequested = true;
        if (mParent instanceof View && !((View) mParent).mForceLayout) ((View) mParent).requestLayout();
    }
    public void forceLayout() { mForceLayout = true; mLayoutRequired = true; }
    public boolean isLayoutRequested() { return mForceLayout; }
    public boolean isInLayout() { return false; }
    public boolean isLaidOut() { return mRight > mLeft || mBottom > mTop; }
    public final int getWidth() { return mRight - mLeft; }
    public final int getHeight() { return mBottom - mTop; }
    public final int getLeft() { return mLeft; }
    public final int getTop() { return mTop; }
    public final int getRight() { return mRight; }
    public final int getBottom() { return mBottom; }
    public final void setLeft(int l) { setFrame(l, mTop, mRight, mBottom); }
    public final void setTop(int t) { setFrame(mLeft, t, mRight, mBottom); }
    public final void setRight(int r) { setFrame(mLeft, mTop, r, mBottom); }
    public final void setBottom(int b) { setFrame(mLeft, mTop, mRight, b); }
    public void offsetLeftAndRight(int d) { mLeft += d; mRight += d; }
    public void offsetTopAndBottom(int d) { mTop += d; mBottom += d; }
    public float getX() { return mLeft + translationX; }
    public float getY() { return mTop + translationY; }
    public void setX(float x) { translationX = x - mLeft; }
    public void setY(float y) { translationY = y - mTop; }
    public void setTranslationX(float x) { translationX = x; }
    public void setTranslationY(float y) { translationY = y; }
    public void setRotation(float r) { rotation = r; }
    public void setScaleX(float s) { scaleX = s; }
    public void setScaleY(float s) { scaleY = s; }
    public float getTranslationX() { return translationX; }
    public float getTranslationY() { return translationY; }
    public float getScaleX() { return scaleX; }
    public float getScaleY() { return scaleY; }
    public float getRotation() { return rotation; }
    public int getScrollX() { return mScrollX; }
    public int getScrollY() { return mScrollY; }
    public void scrollTo(int x, int y) { if (x != mScrollX || y != mScrollY) { int ox = mScrollX, oy = mScrollY; mScrollX = x; mScrollY = y; onScrollChanged(x, y, ox, oy); invalidate(); } }
    public void scrollBy(int x, int y) { scrollTo(mScrollX + x, mScrollY + y); }
    protected void onScrollChanged(int l, int t, int oldl, int oldt) {}
    public void addOnLayoutChangeListener(OnLayoutChangeListener l) { if (layoutListeners == null) layoutListeners = new java.util.ArrayList<OnLayoutChangeListener>(); layoutListeners.add(l); }
    public void removeOnLayoutChangeListener(OnLayoutChangeListener l) { if (layoutListeners != null) layoutListeners.remove(l); }
    public void getLocationOnScreen(int[] out) { getLocationInWindow(out); }
    public void getLocationInWindow(int[] out) {
        int x = 0, y = 0;
        View v = this;
        while (v != null) { x += v.mLeft - (v == this ? 0 : v.mScrollX); y += v.mTop - (v == this ? 0 : v.mScrollY); v = v.mParent instanceof View ? (View) v.mParent : null; }
        out[0] = x; out[1] = y;
    }
    public boolean getGlobalVisibleRect(android.graphics.Rect r) { int[] p = new int[2]; getLocationInWindow(p); r.set(p[0], p[1], p[0] + getWidth(), p[1] + getHeight()); return getWidth() > 0 && getHeight() > 0; }
    public boolean getLocalVisibleRect(android.graphics.Rect r) { r.set(0, 0, getWidth(), getHeight()); return getWidth() > 0 && getHeight() > 0; }
    public void getHitRect(android.graphics.Rect r) { r.set(mLeft, mTop, mRight, mBottom); }
    public void getDrawingRect(android.graphics.Rect r) { r.set(mScrollX, mScrollY, mScrollX + getWidth(), mScrollY + getHeight()); }
    public int getBaseline() { return -1; }
    public ViewTreeObserver getViewTreeObserver() { return new ViewTreeObserver(); }
    public boolean isAttachedToWindow() { return mAttached; }
    public android.os.IBinder getWindowToken() { return null; }
    public android.os.IBinder getApplicationWindowToken() { return null; }
    public void setSoundEffectsEnabled(boolean b) {}
    public void setHapticFeedbackEnabled(boolean b) {}
    public boolean performHapticFeedback(int c) { return false; }
    public void setKeepScreenOn(boolean b) {}
    public void setScrollContainer(boolean b) {}
    public void setVerticalScrollBarEnabled(boolean b) {}
    public void setHorizontalScrollBarEnabled(boolean b) {}
    public View getFocusedChild() { return null; }
    public ViewPropertyAnimator animate() { return new ViewPropertyAnimator(this); }


    // ---------------------------------------------------------------- misc View API
    public static final int LAYER_TYPE_NONE = 0, LAYER_TYPE_SOFTWARE = 1, LAYER_TYPE_HARDWARE = 2;
    public static final int OVER_SCROLL_ALWAYS = 0, OVER_SCROLL_IF_CONTENT_SCROLLS = 1, OVER_SCROLL_NEVER = 2;
    public static final int LAYOUT_DIRECTION_LTR = 0, LAYOUT_DIRECTION_RTL = 1, LAYOUT_DIRECTION_INHERIT = 2, LAYOUT_DIRECTION_LOCALE = 3;
    public static final int TEXT_DIRECTION_INHERIT = 0, TEXT_DIRECTION_FIRST_STRONG = 1, TEXT_DIRECTION_LOCALE = 5;
    public static final int TEXT_ALIGNMENT_INHERIT = 0, TEXT_ALIGNMENT_TEXT_START = 2, TEXT_ALIGNMENT_TEXT_END = 3, TEXT_ALIGNMENT_VIEW_END = 6;
    public static final int FOCUS_BACKWARD = 1, FOCUS_FORWARD = 2, FOCUS_LEFT = 17, FOCUS_UP = 33, FOCUS_RIGHT = 66, FOCUS_DOWN = 130;
    public static final int SCROLL_AXIS_NONE = 0, SCROLL_AXIS_HORIZONTAL = 1, SCROLL_AXIS_VERTICAL = 2;
    public static final int SCROLL_INDICATOR_TOP = 1, SCROLL_INDICATOR_BOTTOM = 2, SCROLL_INDICATOR_LEFT = 4, SCROLL_INDICATOR_RIGHT = 8, SCROLL_INDICATOR_START = 16, SCROLL_INDICATOR_END = 32;
    public static final int SYSTEM_UI_FLAG_VISIBLE = 0, SYSTEM_UI_FLAG_LOW_PROFILE = 1, SYSTEM_UI_FLAG_HIDE_NAVIGATION = 2, SYSTEM_UI_FLAG_FULLSCREEN = 4,
        SYSTEM_UI_FLAG_LAYOUT_STABLE = 0x100, SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION = 0x200, SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN = 0x400, SYSTEM_UI_FLAG_IMMERSIVE = 0x800,
        SYSTEM_UI_FLAG_IMMERSIVE_STICKY = 0x1000, SYSTEM_UI_FLAG_LIGHT_STATUS_BAR = 0x2000, SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR = 0x10;
    public static final int IMPORTANT_FOR_ACCESSIBILITY_AUTO = 0, IMPORTANT_FOR_ACCESSIBILITY_YES = 1, IMPORTANT_FOR_ACCESSIBILITY_NO = 2, IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS = 4;
    public static final int IMPORTANT_FOR_AUTOFILL_AUTO = 0, IMPORTANT_FOR_AUTOFILL_YES = 1, IMPORTANT_FOR_AUTOFILL_NO = 2, IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS = 8;
    public static final int ACCESSIBILITY_LIVE_REGION_NONE = 0, ACCESSIBILITY_LIVE_REGION_POLITE = 1, ACCESSIBILITY_LIVE_REGION_ASSERTIVE = 2;
    public static final int DRAG_FLAG_GLOBAL = 256, FIND_VIEWS_WITH_TEXT = 1, FIND_VIEWS_WITH_CONTENT_DESCRIPTION = 2, SCREEN_STATE_ON = 1;
    public static final android.util.Property<View, Float> ALPHA = new FloatProp("alpha") { public void setValue(View v, float f) { v.setAlpha(f); } public Float get(View v) { return v.getAlpha(); } };
    public static final android.util.Property<View, Float> TRANSLATION_X = new FloatProp("translationX") { public void setValue(View v, float f) { v.setTranslationX(f); } public Float get(View v) { return v.getTranslationX(); } };
    public static final android.util.Property<View, Float> TRANSLATION_Y = new FloatProp("translationY") { public void setValue(View v, float f) { v.setTranslationY(f); } public Float get(View v) { return v.getTranslationY(); } };
    public static final android.util.Property<View, Float> TRANSLATION_Z = new FloatProp("translationZ") { public void setValue(View v, float f) { v.setTranslationZ(f); } public Float get(View v) { return v.getTranslationZ(); } };
    public static final android.util.Property<View, Float> X = new FloatProp("x") { public void setValue(View v, float f) { v.setX(f); } public Float get(View v) { return v.getX(); } };
    public static final android.util.Property<View, Float> Y = new FloatProp("y") { public void setValue(View v, float f) { v.setY(f); } public Float get(View v) { return v.getY(); } };
    public static final android.util.Property<View, Float> ROTATION = new FloatProp("rotation") { public void setValue(View v, float f) { v.setRotation(f); } public Float get(View v) { return v.getRotation(); } };
    public static final android.util.Property<View, Float> SCALE_X = new FloatProp("scaleX") { public void setValue(View v, float f) { v.setScaleX(f); } public Float get(View v) { return v.getScaleX(); } };
    public static final android.util.Property<View, Float> SCALE_Y = new FloatProp("scaleY") { public void setValue(View v, float f) { v.setScaleY(f); } public Float get(View v) { return v.getScaleY(); } };
    abstract static class FloatProp extends android.util.Property<View, Float> {
        FloatProp(String n) { super(Float.class, n); }
        public abstract void setValue(View v, float f);
        public void set(View v, Float f) { setValue(v, f.floatValue()); }
    }

    private float elevation, translationZ, pivotX = Float.NaN, pivotY = Float.NaN, rotationX, rotationY, transitionAlpha = 1f;
    private int layerType, overScrollMode = OVER_SCROLL_IF_CONTENT_SCROLLS, textAlignment = TEXT_ALIGNMENT_GRAVITY, textDirection = TEXT_DIRECTION_FIRST_STRONG,
        layoutDirection = LAYOUT_DIRECTION_LTR, systemUiVisibility, scrollIndicators, nextClusterForwardId = NO_ID, labelFor = NO_ID, nextFocusLeft = NO_ID, nextFocusRight = NO_ID;
    private boolean longClickable, fitsSystemWindows, saveEnabled = true, saveFromParentEnabled = true, willNotDraw, nestedScrolling, transientState, focusedByDefault, keyboardCluster, screenReaderFocusable;
    private CharSequence stateDescription, tooltipText;
    private String transitionName;
    private android.graphics.Rect clipBounds;
    private java.util.List<android.graphics.Rect> gestureExclusion = new java.util.ArrayList<android.graphics.Rect>();
    private android.graphics.PorterDuff.Mode bgTintMode;
    private OnHoverListener hoverListener;
    private android.view.animation.Animation currentAnimation;
    public interface OnHoverListener { boolean onHover(View v, MotionEvent e); }
    public interface OnCreateContextMenuListener { void onCreateContextMenu(ContextMenu menu, View v, ContextMenu.ContextMenuInfo info); }
    public interface OnUnhandledKeyEventListener { boolean onUnhandledKeyEvent(View v, KeyEvent e); }
    public interface OnScrollChangeListener { void onScrollChange(View v, int scrollX, int scrollY, int oldScrollX, int oldScrollY); }
    public interface OnGenericMotionListener { boolean onGenericMotion(View v, MotionEvent e); }
    public interface OnContextClickListener { boolean onContextClick(View v); }
    public interface OnSystemUiVisibilityChangeListener { void onSystemUiVisibilityChange(int visibility); }

    public static class BaseSavedState extends AbsSavedState {
        public BaseSavedState(android.os.Parcel source) { super(source); }
        public BaseSavedState(android.os.Parcel source, ClassLoader loader) { super(source, loader); }
        public BaseSavedState(android.os.Parcelable superState) { super(superState); }
        public static final android.os.Parcelable.Creator<BaseSavedState> CREATOR = new android.os.Parcelable.ClassLoaderCreator<BaseSavedState>() {
            public BaseSavedState createFromParcel(android.os.Parcel in) { return new BaseSavedState(in); }
            public BaseSavedState createFromParcel(android.os.Parcel in, ClassLoader loader) { return new BaseSavedState(in, loader); }
            public BaseSavedState[] newArray(int size) { return new BaseSavedState[size]; }
        };
    }

    public void setElevation(float e) { elevation = e; }
    public float getElevation() { return elevation; }
    public void setTranslationZ(float z) { translationZ = z; }
    public float getTranslationZ() { return translationZ; }
    public float getZ() { return elevation + translationZ; }
    public void setZ(float z) { translationZ = z - elevation; }
    public void setPivotX(float x) { pivotX = x; }
    public void setPivotY(float y) { pivotY = y; }
    public float getPivotX() { return pivotX != pivotX ? getWidth() / 2f : pivotX; }
    public float getPivotY() { return pivotY != pivotY ? getHeight() / 2f : pivotY; }
    public boolean isPivotSet() { return pivotX == pivotX || pivotY == pivotY; }
    public void resetPivot() { pivotX = pivotY = Float.NaN; }
    public void setRotationX(float r) { rotationX = r; }
    public void setRotationY(float r) { rotationY = r; }
    public float getRotationX() { return rotationX; }
    public float getRotationY() { return rotationY; }
    public void setCameraDistance(float d) {}
    public float getCameraDistance() { return 1280; }
    public android.graphics.Matrix getMatrix() {
        android.graphics.Matrix m = new android.graphics.Matrix();
        m.setTranslate(getTranslationX(), getTranslationY());
        m.preRotate(getRotation(), getPivotX(), getPivotY());
        m.preScale(getScaleX(), getScaleY(), getPivotX(), getPivotY());
        return m;
    }
    public void setAnimationMatrix(android.graphics.Matrix m) {}
    public void transformMatrixToGlobal(android.graphics.Matrix m) { int[] p = new int[2]; getLocationInWindow(p); m.preTranslate(p[0], p[1]); }
    public void transformMatrixToLocal(android.graphics.Matrix m) { int[] p = new int[2]; getLocationInWindow(p); m.postTranslate(-p[0], -p[1]); }
    public void setTransitionAlpha(float a) { transitionAlpha = a; }
    public float getTransitionAlpha() { return transitionAlpha; }
    public void setTransitionName(String n) { transitionName = n; }
    public String getTransitionName() { return transitionName; }
    public void setTransitionVisibility(int v) { visibility = v; }
    public void setLayerType(int type, android.graphics.Paint p) { layerType = type; }
    public int getLayerType() { return layerType; }
    public void setLayerPaint(android.graphics.Paint p) {}
    public boolean hasOverlappingRendering() { return true; }
    public void setWillNotDraw(boolean b) { willNotDraw = b; }
    public boolean willNotDraw() { return willNotDraw; }
    public void setWillNotCacheDrawing(boolean b) {}
    public void setDrawingCacheEnabled(boolean b) {}
    public void buildDrawingCache() {}
    public void destroyDrawingCache() {}
    public android.graphics.Bitmap getDrawingCache() { return null; }
    public void setClipBounds(android.graphics.Rect r) { clipBounds = r == null ? null : new android.graphics.Rect(r); invalidate(); }
    public android.graphics.Rect getClipBounds() { return clipBounds == null ? null : new android.graphics.Rect(clipBounds); }
    public boolean getClipBounds(android.graphics.Rect out) { if (clipBounds == null) return false; out.set(clipBounds); return true; }
    public void setClipToOutline(boolean b) {}
    public boolean getClipToOutline() { return false; }
    public void invalidateOutline() {}
    public void setBackgroundTintMode(android.graphics.PorterDuff.Mode m) { bgTintMode = m; if (mBackground != null) mBackground.setTintMode(m); }
    public android.graphics.PorterDuff.Mode getBackgroundTintMode() { return bgTintMode; }
    public void setForegroundTintList(android.content.res.ColorStateList t) { if (mForeground != null) mForeground.setTintList(t); }
    public void setForegroundGravity(int g) {}
    public void setOverScrollMode(int m) { overScrollMode = m; }
    public int getOverScrollMode() { return overScrollMode; }
    public void setTextAlignment(int a) { textAlignment = a; }
    public int getTextAlignment() { return textAlignment; }
    public void setTextDirection(int d) { textDirection = d; }
    public int getTextDirection() { return textDirection; }
    public void setLayoutDirection(int d) { layoutDirection = d == LAYOUT_DIRECTION_RTL ? 1 : 0; }
    public int getLayoutDirection() { return layoutDirection; }
    public boolean isLayoutRtl() { return layoutDirection == 1; }
    public void onRtlPropertiesChanged(int dir) {}
    public boolean isPaddingRelative() { return false; }
    public boolean canResolveLayoutDirection() { return true; }
    public void setSystemUiVisibility(int v) { systemUiVisibility = v; }
    public int getSystemUiVisibility() { return systemUiVisibility; }
    public int getWindowSystemUiVisibility() { return systemUiVisibility; }
    public void onWindowSystemUiVisibilityChanged(int v) {}
    public void setOnSystemUiVisibilityChangeListener(OnSystemUiVisibilityChangeListener l) {}
    public void setFitsSystemWindows(boolean b) { fitsSystemWindows = b; }
    public boolean getFitsSystemWindows() { return fitsSystemWindows; }
    protected boolean fitSystemWindows(android.graphics.Rect insets) { return false; }
    public void getWindowVisibleDisplayFrame(android.graphics.Rect r) { View root = getRootView(); r.set(0, 0, root.getWidth(), root.getHeight()); }
    public int getWindowVisibility() { return mAttached ? VISIBLE : GONE; }
    protected void onWindowVisibilityChanged(int v) {}
    public void onWindowFocusChanged(boolean b) {}
    public boolean hasWindowFocus() { return mAttached; }
    protected void onVisibilityChanged(View changed, int v) {}
    public void onVisibilityAggregated(boolean visible) {}
    protected void onConfigurationChanged(android.content.res.Configuration c) {}
    protected void onFinishInflate() {}
    public boolean isInEditMode() { return false; }
    public boolean isInTouchMode() { return true; }
    public long getDrawingTime() { return android.os.SystemClock.uptimeMillis(); }
    public void setLongClickable(boolean c) { longClickable = c; }
    public boolean isLongClickable() { return longClickable || longClick != null; }
    public void setContextClickable(boolean b) {}
    public void setSaveEnabled(boolean b) { saveEnabled = b; }
    public boolean isSaveEnabled() { return saveEnabled; }
    public void setSaveFromParentEnabled(boolean b) { saveFromParentEnabled = b; }
    public boolean isSaveFromParentEnabled() { return saveFromParentEnabled; }
    protected android.os.Parcelable onSaveInstanceState() { return AbsSavedState.EMPTY_STATE; }
    protected void onRestoreInstanceState(android.os.Parcelable state) {}
    public void saveHierarchyState(android.util.SparseArray<android.os.Parcelable> c) { dispatchSaveInstanceState(c); }
    public void restoreHierarchyState(android.util.SparseArray<android.os.Parcelable> c) { dispatchRestoreInstanceState(c); }
    protected void dispatchSaveInstanceState(android.util.SparseArray<android.os.Parcelable> c) { if (getId() != NO_ID && saveEnabled) c.put(getId(), onSaveInstanceState()); }
    protected void dispatchRestoreInstanceState(android.util.SparseArray<android.os.Parcelable> c) { if (getId() != NO_ID) { android.os.Parcelable p = c.get(getId()); if (p != null) onRestoreInstanceState(p); } }
    public void setHasTransientState(boolean b) { transientState = b; }
    public boolean hasTransientState() { return transientState; }
    public void setDuplicateParentStateEnabled(boolean b) {}
    public boolean isDuplicateParentStateEnabled() { return false; }
    public void setStateDescription(CharSequence d) { stateDescription = d; }
    public CharSequence getStateDescription() { return stateDescription; }
    public void setTooltipText(CharSequence t) { tooltipText = t; }
    public CharSequence getTooltipText() { return tooltipText; }
    public void setLabelFor(int id) { labelFor = id; }
    public int getLabelFor() { return labelFor; }
    public void setScreenReaderFocusable(boolean b) { screenReaderFocusable = b; }
    public boolean isScreenReaderFocusable() { return screenReaderFocusable; }
    public void setScrollIndicators(int i) { scrollIndicators = i; }
    public void setScrollIndicators(int i, int mask) { scrollIndicators = (scrollIndicators & ~mask) | (i & mask); }
    public int getScrollIndicators() { return scrollIndicators; }
    public void setSystemGestureExclusionRects(java.util.List<android.graphics.Rect> r) { gestureExclusion = new java.util.ArrayList<android.graphics.Rect>(r); }
    public java.util.List<android.graphics.Rect> getSystemGestureExclusionRects() { return gestureExclusion; }
    public int getVerticalFadingEdgeLength() { return 0; }
    public int getHorizontalFadingEdgeLength() { return 0; }
    public void setVerticalFadingEdgeEnabled(boolean b) {}
    public void setHorizontalFadingEdgeEnabled(boolean b) {}
    public void setFadingEdgeLength(int l) {}
    public void setScrollBarStyle(int s) {}
    public void setScrollbarFadingEnabled(boolean b) {}
    protected boolean awakenScrollBars() { return false; }
    protected int computeHorizontalScrollRange() { return getWidth(); }
    protected int computeHorizontalScrollOffset() { return mScrollX; }
    protected int computeHorizontalScrollExtent() { return getWidth(); }
    protected int computeVerticalScrollRange() { return getHeight(); }
    protected int computeVerticalScrollOffset() { return mScrollY; }
    protected int computeVerticalScrollExtent() { return getHeight(); }
    public boolean canScrollHorizontally(int dir) {
        int offset = computeHorizontalScrollOffset(), range = computeHorizontalScrollRange() - computeHorizontalScrollExtent();
        if (range == 0) return false;
        return dir < 0 ? offset > 0 : offset < range - 1;
    }
    public boolean canScrollVertically(int dir) {
        int offset = computeVerticalScrollOffset(), range = computeVerticalScrollRange() - computeVerticalScrollExtent();
        if (range == 0) return false;
        return dir < 0 ? offset > 0 : offset < range - 1;
    }
    public void setNestedScrollingEnabled(boolean b) { nestedScrolling = b; }
    public boolean isNestedScrollingEnabled() { return nestedScrolling; }
    public boolean startNestedScroll(int axes) { return false; }
    public void stopNestedScroll() {}
    public boolean hasNestedScrollingParent() { return false; }
    public boolean dispatchNestedScroll(int dxC, int dyC, int dxU, int dyU, int[] offset) { return false; }
    public boolean dispatchNestedPreScroll(int dx, int dy, int[] consumed, int[] offset) { return false; }
    public boolean dispatchNestedFling(float vx, float vy, boolean consumed) { return false; }
    public boolean dispatchNestedPreFling(float vx, float vy) { return false; }
    public void setOnScrollChangeListener(OnScrollChangeListener l) {}
    public void bringToFront() { if (mParent instanceof ViewGroup) ((ViewGroup) mParent).bringChildToFront(this); }
    public void cancelPendingInputEvents() {}
    public void cancelLongPress() {}
    public void playSoundEffect(int s) {}
    public void postOnAnimationDelayed(Runnable r, long ms) { postDelayed(r, ms + 16); }
    public void requestApplyInsets() { requestLayout(); }
    public boolean requestRectangleOnScreen(android.graphics.Rect r) { return false; }
    public boolean requestRectangleOnScreen(android.graphics.Rect r, boolean immediate) { return false; }
    public boolean getGlobalVisibleRect(android.graphics.Rect r, android.graphics.Point offset) {
        boolean v = getGlobalVisibleRect(r);
        if (offset != null) offset.set(r.left, r.top);
        return v;
    }
    public void getFocusedRect(android.graphics.Rect r) { getDrawingRect(r); }
    // focus: the host owns keyboard focus; Java keeps a simple model
    public final boolean requestFocus(int direction) { return requestFocus(); }
    public boolean requestFocus(int direction, android.graphics.Rect previous) { return requestFocus(); }
    public final boolean requestFocusFromTouch() { return requestFocus(); }
    public View findFocus() { return null; }
    public boolean hasFocusable() { return focusable && visibility == VISIBLE; }
    public boolean hasExplicitFocusable() { return hasFocusable(); }
    public java.util.ArrayList<View> getFocusables(int dir) { java.util.ArrayList<View> l = new java.util.ArrayList<View>(); addFocusables(l, dir); return l; }
    public void addFocusables(java.util.ArrayList<View> views, int dir) { addFocusables(views, dir, 0); }
    public void addFocusables(java.util.ArrayList<View> views, int dir, int mode) { if (hasFocusable()) views.add(this); }
    public void addTouchables(java.util.ArrayList<View> views) { if (clickable) views.add(this); }
    protected void onFocusChanged(boolean gain, int dir, android.graphics.Rect prev) { if (focusListener != null) focusListener.onFocusChange(this, gain); }
    public boolean restoreDefaultFocus() { return requestFocus(); }
    public void setFocusedByDefault(boolean b) { focusedByDefault = b; }
    public boolean isFocusedByDefault() { return focusedByDefault; }
    public void setKeyboardNavigationCluster(boolean b) { keyboardCluster = b; }
    public boolean isKeyboardNavigationCluster() { return keyboardCluster; }
    public View keyboardNavigationClusterSearch(View current, int dir) { return null; }
    public void addKeyboardNavigationClusters(java.util.Collection<View> views, int dir) {}
    public void setNextClusterForwardId(int id) { nextClusterForwardId = id; }
    public int getNextClusterForwardId() { return nextClusterForwardId; }
    public void setNextFocusLeftId(int id) { nextFocusLeft = id; }
    public void setNextFocusRightId(int id) { nextFocusRight = id; }
    public void setNextFocusUpId(int id) {}
    public void setNextFocusDownId(int id) {}
    public void setNextFocusForwardId(int id) {}
    public int getNextFocusLeftId() { return nextFocusLeft; }
    public int getNextFocusRightId() { return nextFocusRight; }
    // keys
    public boolean dispatchKeyEvent(KeyEvent e) { return onKeyDown(e.getKeyCode(), e); }
    public boolean dispatchKeyEventPreIme(KeyEvent e) { return false; }
    public boolean dispatchKeyShortcutEvent(KeyEvent e) { return false; }
    public boolean onKeyDown(int code, KeyEvent e) { return false; }
    public boolean onKeyUp(int code, KeyEvent e) { return false; }
    public boolean onKeyLongPress(int code, KeyEvent e) { return false; }
    public boolean onKeyMultiple(int code, int count, KeyEvent e) { return false; }
    public boolean onKeyPreIme(int code, KeyEvent e) { return false; }
    public boolean onKeyShortcut(int code, KeyEvent e) { return false; }
    public boolean onCheckIsTextEditor() { return false; }
    public KeyEvent.DispatcherState getKeyDispatcherState() { return null; }
    public void addOnUnhandledKeyEventListener(OnUnhandledKeyEventListener l) {}
    public void removeOnUnhandledKeyEventListener(OnUnhandledKeyEventListener l) {}
    // hover / generic motion
    public void setOnHoverListener(OnHoverListener l) { hoverListener = l; }
    public boolean dispatchHoverEvent(MotionEvent e) { return (hoverListener != null && hoverListener.onHover(this, e)) || onHoverEvent(e); }
    public boolean onHoverEvent(MotionEvent e) { return false; }
    public boolean dispatchGenericMotionEvent(MotionEvent e) { return false; }
    public boolean onGenericMotionEvent(MotionEvent e) { return false; }
    public void setOnGenericMotionListener(OnGenericMotionListener l) {}
    public void setOnCreateContextMenuListener(OnCreateContextMenuListener l) {}
    public void setOnContextClickListener(OnContextClickListener l) {}
    public boolean showContextMenu() { return false; }
    // animation
    public void startAnimation(android.view.animation.Animation a) { currentAnimation = a; if (a != null) { a.start(); } }
    public void setAnimation(android.view.animation.Animation a) { currentAnimation = a; }
    public android.view.animation.Animation getAnimation() { return currentAnimation; }
    public void clearAnimation() { if (currentAnimation != null) currentAnimation.cancel(); currentAnimation = null; }
    public void setLeftTopRightBottom(int l, int t, int r, int b) { setFrame(l, t, r, b); }
    public void saveAttributeDataForStyleable(android.content.Context c, int[] styleable, android.util.AttributeSet attrs, android.content.res.TypedArray t, int defStyleAttr, int defStyleRes) {}


    // ---------------------------------------------------------------- insets, accessibility, autofill
    public interface OnApplyWindowInsetsListener { WindowInsets onApplyWindowInsets(View v, WindowInsets insets); }
    public static class AccessibilityDelegate {
        public void sendAccessibilityEvent(View host, int eventType) {}
        public boolean performAccessibilityAction(View host, int action, android.os.Bundle args) { return false; }
        public void sendAccessibilityEventUnchecked(View host, android.view.accessibility.AccessibilityEvent e) {}
        public boolean dispatchPopulateAccessibilityEvent(View host, android.view.accessibility.AccessibilityEvent e) { return false; }
        public void onPopulateAccessibilityEvent(View host, android.view.accessibility.AccessibilityEvent e) {}
        public void onInitializeAccessibilityEvent(View host, android.view.accessibility.AccessibilityEvent e) {}
        public void onInitializeAccessibilityNodeInfo(View host, android.view.accessibility.AccessibilityNodeInfo info) {}
        public void addExtraDataToAccessibilityNodeInfo(View host, android.view.accessibility.AccessibilityNodeInfo info, String key, android.os.Bundle args) {}
        public boolean onRequestSendAccessibilityEvent(ViewGroup host, View child, android.view.accessibility.AccessibilityEvent e) { return true; }
        public android.view.accessibility.AccessibilityNodeProvider getAccessibilityNodeProvider(View host) { return null; }
    }
    private OnApplyWindowInsetsListener insetsListener;
    private AccessibilityDelegate a11yDelegate;
    private int importantForA11y, importantForAutofill, importantForContentCapture, liveRegion;
    private boolean a11yHeading;
    private CharSequence paneTitle;
    private android.view.autofill.AutofillId autofillId;
    private android.view.contentcapture.ContentCaptureSession captureSession;
    private ViewOutlineProvider outlineProvider = ViewOutlineProvider.BACKGROUND;
    private String[] receiveMimeTypes;
    private OnReceiveContentListener receiveListener;
    public void setOnApplyWindowInsetsListener(OnApplyWindowInsetsListener l) { insetsListener = l; }
    public WindowInsets onApplyWindowInsets(WindowInsets insets) { return insets; }
    /** Insets are always zero: the iOS host lays out the window inside the safe area. */
    public WindowInsets dispatchApplyWindowInsets(WindowInsets insets) {
        return insetsListener != null ? insetsListener.onApplyWindowInsets(this, insets) : onApplyWindowInsets(insets);
    }
    public WindowInsets getRootWindowInsets() { return mAttached ? new WindowInsets(new android.graphics.Rect()) : null; }
    public WindowInsets computeSystemWindowInsets(WindowInsets in, android.graphics.Rect outLocalInsets) { if (outLocalInsets != null) outLocalInsets.setEmpty(); return in; }
    public void setWindowInsetsAnimationCallback(WindowInsetsAnimation.Callback cb) {}
    public android.window.OnBackInvokedDispatcher findOnBackInvokedDispatcher() { return null; }
    public Display getDisplay() { return null; }
    public WindowId getWindowId() { return null; }
    public void setAccessibilityDelegate(AccessibilityDelegate d) { a11yDelegate = d; }
    public AccessibilityDelegate getAccessibilityDelegate() { return a11yDelegate; }
    public void setImportantForAccessibility(int m) { importantForA11y = m; }
    public int getImportantForAccessibility() { return importantForA11y; }
    public boolean isImportantForAccessibility() { return importantForA11y != IMPORTANT_FOR_ACCESSIBILITY_NO && importantForA11y != IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS; }
    public void setAccessibilityLiveRegion(int m) { liveRegion = m; }
    public int getAccessibilityLiveRegion() { return liveRegion; }
    public void setAccessibilityHeading(boolean b) { a11yHeading = b; }
    public boolean isAccessibilityHeading() { return a11yHeading; }
    public void setAccessibilityPaneTitle(CharSequence t) { paneTitle = t; }
    public CharSequence getAccessibilityPaneTitle() { return paneTitle; }
    public void setAccessibilityTraversalBefore(int id) {}
    public void setAccessibilityTraversalAfter(int id) {}
    public void announceForAccessibility(CharSequence text) {}
    public void sendAccessibilityEvent(int type) { if (a11yDelegate != null) a11yDelegate.sendAccessibilityEvent(this, type); }
    public void sendAccessibilityEventUnchecked(android.view.accessibility.AccessibilityEvent e) {}
    public boolean dispatchPopulateAccessibilityEvent(android.view.accessibility.AccessibilityEvent e) { return false; }
    public void onPopulateAccessibilityEvent(android.view.accessibility.AccessibilityEvent e) {}
    public void onInitializeAccessibilityEvent(android.view.accessibility.AccessibilityEvent e) {}
    public void onInitializeAccessibilityNodeInfo(android.view.accessibility.AccessibilityNodeInfo info) {}
    public android.view.accessibility.AccessibilityNodeInfo createAccessibilityNodeInfo() {
        android.view.accessibility.AccessibilityNodeInfo i = android.view.accessibility.AccessibilityNodeInfo.obtain();
        if (a11yDelegate != null) a11yDelegate.onInitializeAccessibilityNodeInfo(this, i); else onInitializeAccessibilityNodeInfo(i);
        return i;
    }
    public android.view.accessibility.AccessibilityNodeProvider getAccessibilityNodeProvider() { return null; }
    public boolean performAccessibilityAction(int action, android.os.Bundle args) { return a11yDelegate != null && a11yDelegate.performAccessibilityAction(this, action, args); }
    public CharSequence getAccessibilityClassName() { return getClass().getName(); }
    public void setImportantForAutofill(int m) { importantForAutofill = m; }
    public int getImportantForAutofill() { return importantForAutofill; }
    public boolean isImportantForAutofill() { return importantForAutofill != IMPORTANT_FOR_AUTOFILL_NO; }
    public void setAutofillHints(String... hints) {}
    public String[] getAutofillHints() { return null; }
    public void setAutofillId(android.view.autofill.AutofillId id) { autofillId = id; }
    public android.view.autofill.AutofillId getAutofillId() { return autofillId; }
    public int getAutofillType() { return 0; }
    public void dispatchProvideAutofillStructure(ViewStructure s, int flags) {}
    public void onProvideAutofillStructure(ViewStructure s, int flags) {}
    public void onProvideStructure(ViewStructure s) {}
    public void setImportantForContentCapture(int m) { importantForContentCapture = m; }
    public int getImportantForContentCapture() { return importantForContentCapture; }
    public boolean isImportantForContentCapture() { return false; }
    public void setContentCaptureSession(android.view.contentcapture.ContentCaptureSession s) { captureSession = s; }
    public android.view.contentcapture.ContentCaptureSession getContentCaptureSession() { return captureSession; }
    public android.view.inputmethod.InputConnection onCreateInputConnection(android.view.inputmethod.EditorInfo info) { return null; }
    public boolean onDragEvent(DragEvent e) { return false; }
    public boolean dispatchDragEvent(DragEvent e) { return onDragEvent(e); }
    public PointerIcon onResolvePointerIcon(MotionEvent e, int pointerIndex) { return null; }
    public void setPointerIcon(PointerIcon icon) {}
    public void setOutlineProvider(ViewOutlineProvider p) { outlineProvider = p; }
    public ViewOutlineProvider getOutlineProvider() { return outlineProvider; }
    public void setOnReceiveContentListener(String[] mimeTypes, OnReceiveContentListener l) { receiveMimeTypes = mimeTypes; receiveListener = l; }
    public String[] getReceiveContentMimeTypes() { return receiveMimeTypes; }
    public ContentInfo performReceiveContent(ContentInfo payload) { return receiveListener != null ? receiveListener.onReceiveContent(this, payload) : onReceiveContent(payload); }
    public ContentInfo onReceiveContent(ContentInfo payload) { return payload; }
    public ViewOverlay getOverlay() { if (overlay == null) overlay = new ViewOverlay(); return overlay; }
    ViewOverlay overlay;

    /** indroid: host type name. */
    protected String indroidType() { return "View"; }
    /** indroid: append type-specific JSON fields (each starting with ','). */
    protected void indroidProps(StringBuilder sb) {}

    public final void indroidJson(StringBuilder sb) {
        sb.append("{\"id\":").append(indroidUid);
        Json.key(sb, "type"); Json.str(sb, indroidType());
        Json.key(sb, "frame"); sb.append('[').append(mLeft).append(',').append(mTop).append(',').append(getWidth()).append(',').append(getHeight()).append(']');
        if (visibility != VISIBLE) { Json.key(sb, "visible"); sb.append(visibility == INVISIBLE ? "\"invisible\"" : "false"); }
        if (pl != 0 || pt != 0 || pr != 0 || pb != 0) { Json.key(sb, "padding"); sb.append('[').append(pl).append(',').append(pt).append(',').append(pr).append(',').append(pb).append(']'); }
        if (mBackground instanceof ColorDrawable) {
            int c = ((ColorDrawable) mBackground).getColor();
            if (c != 0) { Json.key(sb, "bg"); Json.str(sb, android.graphics.Color.toHex(c)); }
        }
        if (alpha != 1f) { Json.key(sb, "alpha"); Json.num(sb, alpha); }
        if (!enabled) { Json.key(sb, "enabled"); sb.append("false"); }
        if (mOnClickListener != null || clickable) { Json.key(sb, "clickable"); sb.append("true"); }
        if (minW > 0) { Json.key(sb, "minWidth"); sb.append(minW); }
        if (minH > 0) { Json.key(sb, "minHeight"); sb.append(minH); }
        ViewGroup.LayoutParams lp = mLayoutParams;
        if (lp != null) {
            Json.key(sb, "width"); dim(sb, lp.width);
            Json.key(sb, "height"); dim(sb, lp.height);
            if (lp instanceof ViewGroup.MarginLayoutParams) {
                ViewGroup.MarginLayoutParams m = (ViewGroup.MarginLayoutParams) lp;
                if (m.leftMargin != 0 || m.topMargin != 0 || m.rightMargin != 0 || m.bottomMargin != 0) {
                    Json.key(sb, "margin"); sb.append('[').append(m.leftMargin).append(',').append(m.topMargin).append(',').append(m.rightMargin).append(',').append(m.bottomMargin).append(']');
                }
            }
            if (lp instanceof android.widget.LinearLayout.LayoutParams) {
                android.widget.LinearLayout.LayoutParams l = (android.widget.LinearLayout.LayoutParams) lp;
                if (l.weight != 0) { Json.key(sb, "weight"); Json.num(sb, l.weight); }
                String g = Gravity.describe(l.gravity);
                if (g != null) { Json.key(sb, "layoutGravity"); Json.str(sb, g); }
            } else if (lp instanceof android.widget.FrameLayout.LayoutParams) {
                String g = Gravity.describe(((android.widget.FrameLayout.LayoutParams) lp).gravity);
                if (g != null) { Json.key(sb, "layoutGravity"); Json.str(sb, g); }
            }
        }
        if (touchListener != null || VMViews.overridesOnTouch(this)) { Json.key(sb, "touch"); sb.append("true"); }
        if (indroidNeedsCanvas()) { Json.key(sb, "draw"); android.graphics.Canvas c = new android.graphics.Canvas(getWidth(),getHeight()); if (visibility == VISIBLE && getWidth() > 0 && getHeight() > 0) draw(c); sb.append(c.indroidOps()); }
        indroidProps(sb);
        sb.append('}');
    }
    private static void dim(StringBuilder sb, int d) {
        if (d == ViewGroup.LayoutParams.MATCH_PARENT) sb.append("\"match\"");
        else if (d == ViewGroup.LayoutParams.WRAP_CONTENT) sb.append("\"wrap\"");
        else sb.append(d);
    }
 public void onProvideAutofillVirtualStructure(ViewStructure structure, int flags) {}
}
