package android.widget;

import android.content.Context;
import android.util.AttributeSet;
import android.view.View;
import android.view.ViewGroup;
import android.view.XmlAttrs;

/** Vertical scroll container. Scrolling itself is native on the host. */
public class ScrollView extends FrameLayout {
    private boolean fillViewport;
    public ScrollView(Context c) { super(c); }
    public ScrollView(Context c, AttributeSet a) { this(c, a, 16842880 /*scrollViewStyle*/); }
    public ScrollView(Context c, AttributeSet a, int s) { super(c, a, s); if (a instanceof XmlAttrs) fillViewport = ((XmlAttrs) a).bool("fillViewport", false); }
    public void setFillViewport(boolean b) { fillViewport = b; requestLayout(); }
    public boolean isFillViewport() { return fillViewport; }
    public void fullScroll(int dir) { View c = getChildAt(0); if (c != null) scrollTo(0, dir == FOCUS_UP ? 0 : Math.max(0, c.getHeight() - getHeight())); }
    public void smoothScrollTo(int x, int y) { scrollTo(x, y); }
    public void smoothScrollBy(int x, int y) { scrollBy(x, y); }
    public boolean pageScroll(int dir) { return false; }
    public boolean arrowScroll(int dir) { return false; }
    public void setSmoothScrollingEnabled(boolean b) {}
    public int getMaxScrollAmount() { return (int) (0.5f * getHeight()); }
    public boolean canScrollVertically(int dir) { View c = getChildAt(0); if (c == null) return false; return dir < 0 ? getScrollY() > 0 : getScrollY() < c.getHeight() - getHeight(); }
    public static final int FOCUS_UP = 33, FOCUS_DOWN = 130;
    public void addView(View child, int index, ViewGroup.LayoutParams p) {
        if (getChildCount() > 0) throw new IllegalStateException("ScrollView can host only one direct child");
        super.addView(child, index, p);
    }
    protected ViewGroup.LayoutParams generateDefaultLayoutParams() { return new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT); }
    protected void measureChild(View child, int ws, int hs) {
        ViewGroup.LayoutParams lp = child.getLayoutParams();
        child.measure(getChildMeasureSpec(ws, getPaddingLeft() + getPaddingRight(), lp.width), MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED));
    }
    protected void measureChildWithMargins(View child, int ws, int wUsed, int hs, int hUsed) {
        MarginLayoutParams lp = (MarginLayoutParams) child.getLayoutParams();
        int cws = getChildMeasureSpec(ws, getPaddingLeft() + getPaddingRight() + lp.leftMargin + lp.rightMargin + wUsed, lp.width);
        int used = getPaddingTop() + getPaddingBottom() + lp.topMargin + lp.bottomMargin + hUsed;
        child.measure(cws, MeasureSpec.makeMeasureSpec(Math.max(0, MeasureSpec.getSize(hs) - used), MeasureSpec.UNSPECIFIED));
    }
    protected void onMeasure(int ws, int hs) {
        super.onMeasure(ws, hs);
        if (!fillViewport || MeasureSpec.getMode(hs) == MeasureSpec.UNSPECIFIED || getChildCount() == 0) return;
        View child = getChildAt(0);
        int height = getMeasuredHeight() - getPaddingTop() - getPaddingBottom();
        MarginLayoutParams lp = (MarginLayoutParams) child.getLayoutParams();
        int avail = height - lp.topMargin - lp.bottomMargin;
        if (child.getMeasuredHeight() < avail) {
            int cws = getChildMeasureSpec(ws, getPaddingLeft() + getPaddingRight() + lp.leftMargin + lp.rightMargin, lp.width);
            child.measure(cws, MeasureSpec.makeMeasureSpec(avail, MeasureSpec.EXACTLY));
        }
    }
    protected String indroidType() { return "ScrollView"; }
    public ScrollView(Context c, AttributeSet a, int s, int r) { this(c,a,s); }
}
