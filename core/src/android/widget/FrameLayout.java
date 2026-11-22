package android.widget;

import android.content.Context;
import android.util.AttributeSet;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.view.XmlAttrs;
import java.util.ArrayList;

public class FrameLayout extends ViewGroup {
    public static class LayoutParams extends ViewGroup.MarginLayoutParams {
        public static final int UNSPECIFIED_GRAVITY = -1;
        public int gravity = UNSPECIFIED_GRAVITY;
        public LayoutParams(int w, int h) { super(w, h); }
        public LayoutParams(int w, int h, int gravity) { super(w, h); this.gravity = gravity; }
        public LayoutParams(ViewGroup.LayoutParams p) { super(p); if (p instanceof LayoutParams) gravity = ((LayoutParams) p).gravity; }
        public LayoutParams(ViewGroup.MarginLayoutParams p) { super(p); }
        public LayoutParams(LayoutParams p) { super(p); gravity = p.gravity; }
        public LayoutParams(Context c, AttributeSet attrs) {
            super(c, attrs);
            if (attrs instanceof XmlAttrs) gravity = ((XmlAttrs) attrs).integer("layout_gravity", -1);
        }
    }
    private final ArrayList<View> matchParentChildren = new ArrayList<View>(1);
    private boolean measureAllChildren;
    private int foregroundGravity;

    public FrameLayout(Context c) { super(c); }
    public FrameLayout(Context c, AttributeSet a) { super(c, a); }
    public FrameLayout(Context c, AttributeSet a, int s) { super(c, a, s); }
    public FrameLayout(Context c, AttributeSet a, int s, int r) { super(c, a, s, r); }
    protected ViewGroup.LayoutParams generateDefaultLayoutParams() { return new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT); }
    public LayoutParams generateLayoutParams(AttributeSet attrs) { return new LayoutParams(getContext(), attrs); }
    protected ViewGroup.LayoutParams generateLayoutParams(ViewGroup.LayoutParams p) {
        if (p instanceof LayoutParams) return new LayoutParams((LayoutParams) p);
        if (p instanceof MarginLayoutParams) return new LayoutParams((MarginLayoutParams) p);
        return new LayoutParams(p);
    }
    protected boolean checkLayoutParams(ViewGroup.LayoutParams p) { return p instanceof LayoutParams; }
    public void setForeground(android.graphics.drawable.Drawable d) {}
    public android.graphics.drawable.Drawable getForeground() { return null; }
    public void setForegroundGravity(int g) { foregroundGravity = g; }
    public void setMeasureAllChildren(boolean b) { measureAllChildren = b; }
    public boolean getMeasureAllChildren() { return measureAllChildren; }
    public boolean getConsiderGoneChildrenWhenMeasuring() { return measureAllChildren; }

    protected void onMeasure(int ws, int hs) {
        int count = getChildCount();
        boolean measureMatchParent = MeasureSpec.getMode(ws) != MeasureSpec.EXACTLY || MeasureSpec.getMode(hs) != MeasureSpec.EXACTLY;
        matchParentChildren.clear();
        int maxH = 0, maxW = 0, childState = 0;
        for (int i = 0; i < count; i++) {
            View child = getChildAt(i);
            if (!measureAllChildren && child.getVisibility() == GONE) continue;
            measureChildWithMargins(child, ws, 0, hs, 0);
            MarginLayoutParams lp = (MarginLayoutParams) child.getLayoutParams();
            maxW = Math.max(maxW, child.getMeasuredWidth() + lp.leftMargin + lp.rightMargin);
            maxH = Math.max(maxH, child.getMeasuredHeight() + lp.topMargin + lp.bottomMargin);
            childState = combineMeasuredStates(childState, child.getMeasuredState());
            if (measureMatchParent && (lp.width == LayoutParams.MATCH_PARENT || lp.height == LayoutParams.MATCH_PARENT)) matchParentChildren.add(child);
        }
        maxW += getPaddingLeft() + getPaddingRight();
        maxH += getPaddingTop() + getPaddingBottom();
        maxH = Math.max(maxH, getSuggestedMinimumHeight());
        maxW = Math.max(maxW, getSuggestedMinimumWidth());
        setMeasuredDimension(resolveSizeAndState(maxW, ws, childState), resolveSizeAndState(maxH, hs, childState << MEASURED_HEIGHT_STATE_SHIFT));
        if (matchParentChildren.size() > 1) {
            for (View child : matchParentChildren) {
                MarginLayoutParams lp = (MarginLayoutParams) child.getLayoutParams();
                int cws = lp.width == LayoutParams.MATCH_PARENT
                        ? MeasureSpec.makeMeasureSpec(Math.max(0, getMeasuredWidth() - getPaddingLeft() - getPaddingRight() - lp.leftMargin - lp.rightMargin), MeasureSpec.EXACTLY)
                        : getChildMeasureSpec(ws, getPaddingLeft() + getPaddingRight() + lp.leftMargin + lp.rightMargin, lp.width);
                int chs = lp.height == LayoutParams.MATCH_PARENT
                        ? MeasureSpec.makeMeasureSpec(Math.max(0, getMeasuredHeight() - getPaddingTop() - getPaddingBottom() - lp.topMargin - lp.bottomMargin), MeasureSpec.EXACTLY)
                        : getChildMeasureSpec(hs, getPaddingTop() + getPaddingBottom() + lp.topMargin + lp.bottomMargin, lp.height);
                child.measure(cws, chs);
            }
        }
    }

    protected void onLayout(boolean changed, int l, int t, int r, int b) { layoutChildren(l, t, r, b); }

    void layoutChildren(int l, int t, int r, int b) {
        int parentLeft = getPaddingLeft(), parentRight = r - l - getPaddingRight();
        int parentTop = getPaddingTop(), parentBottom = b - t - getPaddingBottom();
        for (int i = 0; i < getChildCount(); i++) {
            View child = getChildAt(i);
            if (child.getVisibility() == GONE) continue;
            LayoutParams lp = (LayoutParams) child.getLayoutParams();
            int w = child.getMeasuredWidth(), h = child.getMeasuredHeight();
            int gravity = lp.gravity == -1 ? Gravity.TOP | Gravity.START : lp.gravity;
            int hg = Gravity.getAbsoluteGravity(gravity, 0) & Gravity.HORIZONTAL_GRAVITY_MASK;
            int vg = gravity & Gravity.VERTICAL_GRAVITY_MASK;
            int left, top;
            switch (hg) {
                case Gravity.CENTER_HORIZONTAL: left = parentLeft + (parentRight - parentLeft - w) / 2 + lp.leftMargin - lp.rightMargin; break;
                case Gravity.RIGHT: left = parentRight - w - lp.rightMargin; break;
                default: left = parentLeft + lp.leftMargin;
            }
            switch (vg) {
                case Gravity.CENTER_VERTICAL: top = parentTop + (parentBottom - parentTop - h) / 2 + lp.topMargin - lp.bottomMargin; break;
                case Gravity.BOTTOM: top = parentBottom - h - lp.bottomMargin; break;
                default: top = parentTop + lp.topMargin;
            }
            child.layout(left, top, left + w, top + h);
        }
    }
    protected String indroidType() { return "FrameLayout"; }
}
