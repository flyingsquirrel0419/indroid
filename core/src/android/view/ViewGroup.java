package android.view;

import android.content.Context;
import android.util.AttributeSet;
import java.util.ArrayList;

public abstract class ViewGroup extends View implements ViewParent, ViewManager {
    public static class LayoutParams {
        public static final int MATCH_PARENT = -1, FILL_PARENT = -1, WRAP_CONTENT = -2;
        public int width, height;
        public LayoutParams(int w, int h) { width = w; height = h; }
        public LayoutParams(LayoutParams src) { width = src.width; height = src.height; }
        public void resolveLayoutDirection(int layoutDirection) {}
        protected void setBaseAttributes(android.content.res.TypedArray a, int widthAttr, int heightAttr) {
            width = a.getLayoutDimension(widthAttr, "layout_width");
            height = a.getLayoutDimension(heightAttr, "layout_height");
        }
        public String debug(String output) { return output + "ViewGroup.LayoutParams={ width=" + width + ", height=" + height + " }"; }
        public LayoutParams(Context c, AttributeSet attrs) {
            XmlAttrs a = attrs instanceof XmlAttrs ? (XmlAttrs) attrs : XmlAttrs.EMPTY;
            width = (int) a.dimension("layout_width", WRAP_CONTENT);
            height = (int) a.dimension("layout_height", WRAP_CONTENT);
        }
    }
    public static class MarginLayoutParams extends LayoutParams {
        public int leftMargin, topMargin, rightMargin, bottomMargin;
        public MarginLayoutParams(int w, int h) { super(w, h); }
        public MarginLayoutParams(MarginLayoutParams src) { this((LayoutParams) src); }
        public MarginLayoutParams(LayoutParams src) {
            super(src);
            if (src instanceof MarginLayoutParams) { MarginLayoutParams m = (MarginLayoutParams) src; setMargins(m.leftMargin, m.topMargin, m.rightMargin, m.bottomMargin); }
        }
        public MarginLayoutParams(Context c, AttributeSet attrs) {
            super(c, attrs);
            XmlAttrs a = attrs instanceof XmlAttrs ? (XmlAttrs) attrs : XmlAttrs.EMPTY;
            int m = (int) a.dimension("layout_margin", 0);
            int mh = (int) a.dimension("layout_marginHorizontal", m), mv = (int) a.dimension("layout_marginVertical", m);
            leftMargin = (int) a.dimension("layout_marginLeft", a.dimension("layout_marginStart", mh));
            rightMargin = (int) a.dimension("layout_marginRight", a.dimension("layout_marginEnd", mh));
            topMargin = (int) a.dimension("layout_marginTop", mv);
            bottomMargin = (int) a.dimension("layout_marginBottom", mv);
        }
        public void setMargins(int l, int t, int r, int b) { leftMargin = l; topMargin = t; rightMargin = r; bottomMargin = b; }
        public void setMarginStart(int s) { leftMargin = s; }
        public void setMarginEnd(int e) { rightMargin = e; }
        public int getMarginStart() { return leftMargin; }
        public int getMarginEnd() { return rightMargin; }
        public boolean isMarginRelative() { return false; }
        public void setLayoutDirection(int d) {}
        public int getLayoutDirection() { return 0; }
        public void resolveLayoutDirection(int layoutDirection) {}
    }
    public interface OnHierarchyChangeListener { void onChildViewAdded(View parent, View child); void onChildViewRemoved(View parent, View child); }

    private final ArrayList<View> children = new ArrayList<View>();

    public ViewGroup(Context c) { super(c); }
    public ViewGroup(Context c, AttributeSet a) { super(c, a); }
    public ViewGroup(Context c, AttributeSet a, int s) { super(c, a, s); }
    public ViewGroup(Context c, AttributeSet a, int s, int r) { super(c, a, s, r); }

    // ---------------------------------------------------------------- measure/layout
    protected abstract void onLayout(boolean changed, int l, int t, int r, int b);

    public static int getChildMeasureSpec(int spec, int padding, int childDimension) {
        int specMode = MeasureSpec.getMode(spec), specSize = MeasureSpec.getSize(spec);
        int size = Math.max(0, specSize - padding);
        int resultSize = 0, resultMode = 0;
        switch (specMode) {
            case MeasureSpec.EXACTLY:
                if (childDimension >= 0) { resultSize = childDimension; resultMode = MeasureSpec.EXACTLY; }
                else if (childDimension == LayoutParams.MATCH_PARENT) { resultSize = size; resultMode = MeasureSpec.EXACTLY; }
                else if (childDimension == LayoutParams.WRAP_CONTENT) { resultSize = size; resultMode = MeasureSpec.AT_MOST; }
                break;
            case MeasureSpec.AT_MOST:
                if (childDimension >= 0) { resultSize = childDimension; resultMode = MeasureSpec.EXACTLY; }
                else if (childDimension == LayoutParams.MATCH_PARENT || childDimension == LayoutParams.WRAP_CONTENT) { resultSize = size; resultMode = MeasureSpec.AT_MOST; }
                break;
            default:
                if (childDimension >= 0) { resultSize = childDimension; resultMode = MeasureSpec.EXACTLY; }
                else { resultSize = size; resultMode = MeasureSpec.UNSPECIFIED; }
                break;
        }
        return MeasureSpec.makeMeasureSpec(resultSize, resultMode);
    }
    protected void measureChildren(int ws, int hs) {
        for (int i = 0; i < children.size(); i++) { View c = children.get(i); if (c.getVisibility() != GONE) measureChild(c, ws, hs); }
    }
    protected void measureChild(View child, int ws, int hs) {
        LayoutParams lp = child.getLayoutParams();
        child.measure(getChildMeasureSpec(ws, pl + pr, lp.width), getChildMeasureSpec(hs, pt + pb, lp.height));
    }
    protected void measureChildWithMargins(View child, int ws, int widthUsed, int hs, int heightUsed) {
        LayoutParams p = child.getLayoutParams();
        int ml = 0, mr = 0, mt = 0, mb = 0;
        if (p instanceof MarginLayoutParams) { MarginLayoutParams m = (MarginLayoutParams) p; ml = m.leftMargin; mr = m.rightMargin; mt = m.topMargin; mb = m.bottomMargin; }
        child.measure(getChildMeasureSpec(ws, pl + pr + ml + mr + widthUsed, p.width), getChildMeasureSpec(hs, pt + pb + mt + mb + heightUsed, p.height));
    }
    public void requestChildFocus(View child, View focused) {}
    public void childDrawableStateChanged(View child) {}
    public void focusableViewAvailable(View v) {}
    public boolean showContextMenuForChild(View v) { return false; }
    public void recomputeViewAttributes(View child) {}
    public void clearChildFocus(View child) {}
    public void invalidateChild(View child, android.graphics.Rect r) { invalidate(); }
    public boolean getChildVisibleRect(View child, android.graphics.Rect r, android.graphics.Point offset) { return true; }
    public boolean requestChildRectangleOnScreen(View child, android.graphics.Rect rect, boolean immediate) { return false; }
    public ViewParent invalidateChildInParent(int[] location, android.graphics.Rect r) { return getParent(); }
    public void requestTransparentRegion(View child) {}
    public void childHasTransientStateChanged(View child, boolean hasTransientState) {}
    public void notifySubtreeAccessibilityStateChanged(View child, View source, int changeType) {}
    public boolean canResolveLayoutDirection() { return true; }
    public void onDescendantInvalidated(View child, View target) {}
    protected boolean addViewInLayout(View child, int index, LayoutParams params) { addView(child, index, params); return true; }
    protected boolean addViewInLayout(View child, int index, LayoutParams params, boolean preventRequestLayout) { addView(child, index, params); return true; }
    protected void attachViewToParent(View child, int index, LayoutParams params) { addView(child, index, params); }
    protected void detachViewFromParent(View child) { removeView(child); }
    protected void detachViewFromParent(int index) { removeViewAt(index); }
    protected void detachAllViewsFromParent() { removeAllViews(); }
    protected void removeDetachedView(View child, boolean animate) {}
    public void removeViewInLayout(View v) { removeView(v); }
    public void removeViewsInLayout(int start, int count) { removeViews(start, count); }
    public void startViewTransition(View v) {}
    public void endViewTransition(View v) {}
    protected void dispatchDraw(android.graphics.Canvas c) {}
    protected boolean drawChild(android.graphics.Canvas c, View child, long time) { return false; }
    public boolean onInterceptTouchEvent(MotionEvent e) { return false; }
    public boolean indroidHasTouchTarget() { return touchTarget != null; }
    public View focusSearch(View focused, int direction) { return null; }

    protected LayoutParams generateDefaultLayoutParams() { return new LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT); }
    public LayoutParams generateLayoutParams(AttributeSet attrs) { return new LayoutParams(getContext(), attrs); }
    protected LayoutParams generateLayoutParams(LayoutParams p) { return p; }
    protected boolean checkLayoutParams(LayoutParams p) { return p != null; }

    public void addView(View child) { addView(child, -1); }
    public void addView(View child, int index) {
        LayoutParams p = child.getLayoutParams();
        if (p == null) p = generateDefaultLayoutParams();
        addView(child, index, p);
    }
    public void addView(View child, int w, int h) { LayoutParams p = generateDefaultLayoutParams(); p.width = w; p.height = h; addView(child, -1, p); }
    public void addView(View child, LayoutParams p) { addView(child, -1, p); }
    public void addView(View child, int index, LayoutParams p) {
        if (child.mParent != null) throw new IllegalStateException("The specified child already has a parent. You must call removeView() on the child's parent first.");
        if (!checkLayoutParams(p)) p = generateLayoutParams(p);
        child.setLayoutParams(p);
        child.mParent = this;
        if (index < 0 || index > children.size()) children.add(child); else children.add(index, child);
        child.mForceLayout = true;
        requestLayout();
        onViewAdded(child);
        child.indroidAttached();
    }
    public void updateViewLayout(View v, LayoutParams p) { v.setLayoutParams(p); requestLayout(); }
    public void removeView(View v) { if (children.remove(v)) { v.mParent = null; requestLayout(); v.indroidDetached(); } }
    public void removeViewAt(int i) { View v = children.remove(i); v.mParent = null; requestLayout(); v.indroidDetached(); }
    public void removeViews(int start, int count) { for (int i = 0; i < count; i++) removeViewAt(start); }
    public void removeAllViews() { java.util.ArrayList<View> old = new java.util.ArrayList<View>(children); children.clear(); for (View v : old) { v.mParent = null; v.indroidDetached(); } requestLayout(); }
    public void removeAllViewsInLayout() { removeAllViews(); }
    public int getChildCount() { return children.size(); }
    public View getChildAt(int i) { return i < 0 || i >= children.size() ? null : children.get(i); }
    public int indexOfChild(View v) { return children.indexOf(v); }
    public void bringChildToFront(View v) { if (children.remove(v)) children.add(v); }
    public void setOnHierarchyChangeListener(OnHierarchyChangeListener l) {}
    public void setClipChildren(boolean b) { clipChildren = b; }
    public void setClipToPadding(boolean b) { clipToPadding = b; }
    public void setDescendantFocusability(int f) { descendantFocusability = f; }
    // ---------------------------------------------------------------- touch dispatch (single pointer)
    private View touchTarget;
    private boolean disallowIntercept;
    public void requestDisallowInterceptTouchEvent(boolean b) {
        disallowIntercept = b;
        if (getParent() != null) getParent().requestDisallowInterceptTouchEvent(b);
    }
    public boolean dispatchTouchEvent(MotionEvent ev) {
        int action = ev.getActionMasked();
        if (action == MotionEvent.ACTION_DOWN) { touchTarget = null; disallowIntercept = false; }
        boolean intercepted = false;
        if (action == MotionEvent.ACTION_DOWN || touchTarget != null) {
            intercepted = !disallowIntercept && onInterceptTouchEvent(ev);
        } else {
            intercepted = true;
        }
        if (!intercepted && action == MotionEvent.ACTION_DOWN) {
            float x = ev.getX() + mScrollX, y = ev.getY() + mScrollY;
            for (int i = children.size() - 1; i >= 0; i--) {
                View c = children.get(i);
                if (c.getVisibility() != VISIBLE) continue;
                float lx = x - c.mLeft - c.getTranslationX(), ly = y - c.mTop - c.getTranslationY();
                if (lx < 0 || ly < 0 || lx >= c.getWidth() || ly >= c.getHeight()) continue;
                if (dispatchToChild(ev, c, false)) { touchTarget = c; return true; }
            }
        }
        if (touchTarget == null) return super.dispatchTouchEvent(ev);
        if (intercepted) {
            View t = touchTarget;
            touchTarget = null;
            dispatchToChild(ev, t, true);
            return true;
        }
        boolean handled = dispatchToChild(ev, touchTarget, false);
        if (action == MotionEvent.ACTION_UP || action == MotionEvent.ACTION_CANCEL) touchTarget = null;
        return handled;
    }
    private boolean dispatchToChild(MotionEvent ev, View c, boolean cancel) {
        MotionEvent e = MotionEvent.obtain(ev);
        if (cancel) e.setAction(MotionEvent.ACTION_CANCEL);
        e.offsetLocation(mScrollX - c.mLeft - c.getTranslationX(), mScrollY - c.mTop - c.getTranslationY());
        boolean r = c.dispatchTouchEvent(e);
        e.recycle();
        return r;
    }
    public boolean shouldDelayChildPressedState() { return false; }

    View findViewTraversal(int id) {
        if (id == getId()) return this;
        for (int i = 0; i < children.size(); i++) { View v = children.get(i).findViewTraversal(id); if (v != null) return v; }
        return null;
    }
    View findWithTag(Object t) {
        View s = super.findWithTag(t);
        if (s != null) return s;
        for (int i = 0; i < children.size(); i++) { View v = children.get(i).findWithTag(t); if (v != null) return v; }
        return null;
    }
    View findByUid(int uid) {
        if (uid == indroidUid) return this;
        for (int i = 0; i < children.size(); i++) { View v = children.get(i).findByUid(uid); if (v != null) return v; }
        return null;
    }

    public static final int FOCUS_BEFORE_DESCENDANTS = 0x20000, FOCUS_AFTER_DESCENDANTS = 0x40000, FOCUS_BLOCK_DESCENDANTS = 0x60000;
    public static final int CLIP_TO_PADDING_MASK = 0x22, LAYOUT_MODE_CLIP_BOUNDS = 0, LAYOUT_MODE_OPTICAL_BOUNDS = 1;
    private boolean clipToPadding = true, clipChildren = true, drawingOrder, suppressLayout;
    private int descendantFocusability = FOCUS_BEFORE_DESCENDANTS;
    private android.animation.LayoutTransition layoutTransition;
    private ViewGroupOverlay groupOverlay;
    public boolean getClipToPadding() { return clipToPadding; }
    public boolean getClipChildren() { return clipChildren; }
    public int getDescendantFocusability() { return descendantFocusability; }
    public android.animation.LayoutTransition getLayoutTransition() { return layoutTransition; }
    public void setLayoutTransition(android.animation.LayoutTransition t) { layoutTransition = t; }
    public ViewGroupOverlay getOverlay() { if (groupOverlay == null) groupOverlay = new ViewGroupOverlay(); return groupOverlay; }
    public void setChildrenDrawingOrderEnabled(boolean b) { drawingOrder = b; }
    protected boolean isChildrenDrawingOrderEnabled() { return drawingOrder; }
    protected int getChildDrawingOrder(int count, int i) { return i; }
    public final int getChildDrawingOrder(int i) { return getChildDrawingOrder(getChildCount(), i); }
    public void setChildrenDrawingCacheEnabled(boolean b) {}
    public void setAddStatesFromChildren(boolean b) {}
    public boolean addStatesFromChildren() { return false; }
    public void suppressLayout(boolean b) { suppressLayout = b; if (!b) requestLayout(); }
    public boolean isLayoutSuppressed() { return suppressLayout; }
    public void onViewAdded(View child) {}
    public void onViewRemoved(View child) {}
    protected boolean onRequestFocusInDescendants(int dir, android.graphics.Rect prev) { return false; }
    public ActionMode startActionModeForChild(View original, ActionMode.Callback cb) { return null; }
    public ActionMode startActionModeForChild(View original, ActionMode.Callback cb, int type) { return null; }
    protected void dispatchFreezeSelfOnly(android.util.SparseArray<android.os.Parcelable> c) { super.dispatchSaveInstanceState(c); }
    protected void dispatchThawSelfOnly(android.util.SparseArray<android.os.Parcelable> c) { super.dispatchRestoreInstanceState(c); }
    protected void dispatchSaveInstanceState(android.util.SparseArray<android.os.Parcelable> c) {
        super.dispatchSaveInstanceState(c);
        for (int i = 0; i < children.size(); i++) { View v = children.get(i); if (v.isSaveFromParentEnabled()) v.dispatchSaveInstanceState(c); }
    }
    protected void dispatchRestoreInstanceState(android.util.SparseArray<android.os.Parcelable> c) {
        super.dispatchRestoreInstanceState(c);
        for (int i = 0; i < children.size(); i++) { View v = children.get(i); if (v.isSaveFromParentEnabled()) v.dispatchRestoreInstanceState(c); }
    }
    public WindowInsets dispatchApplyWindowInsets(WindowInsets insets) {
        WindowInsets r = super.dispatchApplyWindowInsets(insets);
        for (int i = 0; i < children.size(); i++) children.get(i).dispatchApplyWindowInsets(r);
        return r;
    }
    /** Converts a rect in a descendant's coordinates to this view's coordinates. */
    public final void offsetDescendantRectToMyCoords(View d, android.graphics.Rect r) {
        View v = d;
        while (v != null && v != this) { r.offset(v.mLeft - (v == d ? 0 : 0), v.mTop); ViewParent p = v.getParent(); if (p instanceof View) { r.offset(-((View) p).mScrollX, -((View) p).mScrollY); v = (View) p; } else v = null; }
    }
    public final void offsetRectIntoDescendantCoords(View d, android.graphics.Rect r) {
        View v = d;
        while (v != null && v != this) { r.offset(-v.mLeft, -v.mTop); ViewParent p = v.getParent(); if (p instanceof View) { r.offset(((View) p).mScrollX, ((View) p).mScrollY); v = (View) p; } else v = null; }
    }

    void attachTree(boolean attach) {
        super.attachTree(attach);
        for (int i = 0; i < children.size(); i++) children.get(i).attachTree(attach);
    }
    protected String indroidType() { return "FrameLayout"; }
    protected void indroidProps(StringBuilder sb) {
        Json.key(sb, "children");
        sb.append('[');
        for (int i = 0; i < children.size(); i++) { if (i > 0) sb.append(','); children.get(i).indroidJson(sb); }
        sb.append(']');
    }
}
