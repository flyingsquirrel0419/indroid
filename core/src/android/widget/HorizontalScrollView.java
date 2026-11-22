package android.widget;

import android.content.Context;
import android.util.AttributeSet;
import android.view.View;
import android.view.ViewGroup;
import android.view.XmlAttrs;

public class HorizontalScrollView extends FrameLayout {
    private boolean fillViewport;
    public HorizontalScrollView(Context c) { super(c); }
    public HorizontalScrollView(Context c, AttributeSet a) { this(c, a, 0); }
    public HorizontalScrollView(Context c, AttributeSet a, int s) { super(c, a, s); if (a instanceof XmlAttrs) fillViewport = ((XmlAttrs) a).bool("fillViewport", false); }
    public void setFillViewport(boolean b) { fillViewport = b; requestLayout(); }
    public boolean isFillViewport() { return fillViewport; }
    public void fullScroll(int dir) {}
    public void smoothScrollTo(int x, int y) { scrollTo(x, y); }
    public void smoothScrollBy(int x, int y) { scrollBy(x, y); }
    public void setSmoothScrollingEnabled(boolean b) {}
    protected ViewGroup.LayoutParams generateDefaultLayoutParams() { return new LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.MATCH_PARENT); }
    protected void measureChild(View child, int ws, int hs) {
        ViewGroup.LayoutParams lp = child.getLayoutParams();
        child.measure(MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED), getChildMeasureSpec(hs, getPaddingTop() + getPaddingBottom(), lp.height));
    }
    protected void measureChildWithMargins(View child, int ws, int wUsed, int hs, int hUsed) {
        MarginLayoutParams lp = (MarginLayoutParams) child.getLayoutParams();
        int chs = getChildMeasureSpec(hs, getPaddingTop() + getPaddingBottom() + lp.topMargin + lp.bottomMargin + hUsed, lp.height);
        int used = getPaddingLeft() + getPaddingRight() + lp.leftMargin + lp.rightMargin + wUsed;
        child.measure(MeasureSpec.makeMeasureSpec(Math.max(0, MeasureSpec.getSize(ws) - used), MeasureSpec.UNSPECIFIED), chs);
    }
    protected void onMeasure(int ws, int hs) {
        super.onMeasure(ws, hs);
        if (!fillViewport || MeasureSpec.getMode(ws) == MeasureSpec.UNSPECIFIED || getChildCount() == 0) return;
        View child = getChildAt(0);
        MarginLayoutParams lp = (MarginLayoutParams) child.getLayoutParams();
        int avail = getMeasuredWidth() - getPaddingLeft() - getPaddingRight() - lp.leftMargin - lp.rightMargin;
        if (child.getMeasuredWidth() < avail) {
            int chs = getChildMeasureSpec(hs, getPaddingTop() + getPaddingBottom() + lp.topMargin + lp.bottomMargin, lp.height);
            child.measure(MeasureSpec.makeMeasureSpec(avail, MeasureSpec.EXACTLY), chs);
        }
    }
    protected String indroidType() { return "HorizontalScrollView"; }
    public HorizontalScrollView(Context c, AttributeSet a, int s, int r) { this(c,a,s); }
}
