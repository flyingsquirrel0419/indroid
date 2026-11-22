package android.widget;

import android.content.Context;
import android.util.AttributeSet;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.view.XmlAttrs;
import java.util.ArrayList;

/** RelativeLayout with Android's rule semantics (LTR; baseline treated as top). */
public class RelativeLayout extends ViewGroup {
    public static final int TRUE = -1;
    public static final int LEFT_OF = 0, RIGHT_OF = 1, ABOVE = 2, BELOW = 3, ALIGN_BASELINE = 4, ALIGN_LEFT = 5, ALIGN_TOP = 6,
        ALIGN_RIGHT = 7, ALIGN_BOTTOM = 8, ALIGN_PARENT_LEFT = 9, ALIGN_PARENT_TOP = 10, ALIGN_PARENT_RIGHT = 11,
        ALIGN_PARENT_BOTTOM = 12, CENTER_IN_PARENT = 13, CENTER_HORIZONTAL = 14, CENTER_VERTICAL = 15, START_OF = 16,
        END_OF = 17, ALIGN_START = 18, ALIGN_END = 19, ALIGN_PARENT_START = 20, ALIGN_PARENT_END = 21;
    private static final int VERB_COUNT = 22, NOT_SET = Integer.MIN_VALUE;
    private static final String[] XML_RULES = {
        "layout_toLeftOf", "layout_toRightOf", "layout_above", "layout_below", "layout_alignBaseline", "layout_alignLeft",
        "layout_alignTop", "layout_alignRight", "layout_alignBottom", "layout_alignParentLeft", "layout_alignParentTop",
        "layout_alignParentRight", "layout_alignParentBottom", "layout_centerInParent", "layout_centerHorizontal",
        "layout_centerVertical", "layout_toStartOf", "layout_toEndOf", "layout_alignStart", "layout_alignEnd",
        "layout_alignParentStart", "layout_alignParentEnd" };

    public static class LayoutParams extends ViewGroup.MarginLayoutParams {
        private final int[] rules = new int[VERB_COUNT];
        int mLeft = NOT_SET, mTop = NOT_SET, mRight = NOT_SET, mBottom = NOT_SET;
        public boolean alignWithParent;
        public LayoutParams(int w, int h) { super(w, h); }
        public LayoutParams(ViewGroup.LayoutParams p) { super(p); }
        public LayoutParams(ViewGroup.MarginLayoutParams p) { super(p); }
        public LayoutParams(LayoutParams p) { super(p); System.arraycopy(p.rules, 0, rules, 0, VERB_COUNT); alignWithParent = p.alignWithParent; }
        public LayoutParams(Context c, AttributeSet attrs) {
            super(c, attrs);
            if (attrs instanceof XmlAttrs) {
                XmlAttrs a = (XmlAttrs) attrs;
                for (int i = 0; i < VERB_COUNT; i++) {
                    String raw = a.raw(XML_RULES[i]);
                    if (raw == null) continue;
                    if (raw.startsWith("b:")) rules[i] = raw.equals("b:true") ? TRUE : 0;
                    else rules[i] = a.integer(XML_RULES[i], 0);
                }
                alignWithParent = a.bool("layout_alignWithParentIfMissing", false);
            }
        }
        public void addRule(int verb) { addRule(verb, TRUE); }
        public void addRule(int verb, int subject) { rules[verb] = subject; }
        public void removeRule(int verb) { rules[verb] = 0; }
        public int getRule(int verb) { return rules[verb]; }
        public int[] getRules() { return resolved(); }
        public int[] getRules(int layoutDirection) { return resolved(); }
        /** Start/end rules mapped to left/right (LTR). */
        int[] resolved() {
            int[] r = rules.clone();
            if (r[START_OF] != 0) r[LEFT_OF] = r[START_OF];
            if (r[END_OF] != 0) r[RIGHT_OF] = r[END_OF];
            if (r[ALIGN_START] != 0) r[ALIGN_LEFT] = r[ALIGN_START];
            if (r[ALIGN_END] != 0) r[ALIGN_RIGHT] = r[ALIGN_END];
            if (r[ALIGN_PARENT_START] != 0) r[ALIGN_PARENT_LEFT] = r[ALIGN_PARENT_START];
            if (r[ALIGN_PARENT_END] != 0) r[ALIGN_PARENT_RIGHT] = r[ALIGN_PARENT_END];
            if (r[ALIGN_BASELINE] != 0 && r[ALIGN_TOP] == 0) r[ALIGN_TOP] = r[ALIGN_BASELINE];
            return r;
        }
    }

    private int gravity = Gravity.START | Gravity.TOP;
    private int ignoreGravity = View.NO_ID;

    public RelativeLayout(Context c) { super(c); }
    public RelativeLayout(Context c, AttributeSet a) { this(c, a, 0); }
    public RelativeLayout(Context c, AttributeSet a, int s) { this(c, a, s, 0); }
    public RelativeLayout(Context c, AttributeSet a, int s, int r) {
        super(c, a, s, r);
        if (a instanceof XmlAttrs) { int g = ((XmlAttrs) a).integer("gravity", 0); if (g != 0) gravity = g; }
    }
    public void setGravity(int g) { gravity = g; requestLayout(); }
    public int getGravity() { return gravity; }
    public void setHorizontalGravity(int g) { setGravity((gravity & ~Gravity.RELATIVE_HORIZONTAL_GRAVITY_MASK) | g); }
    public void setVerticalGravity(int g) { setGravity((gravity & ~Gravity.VERTICAL_GRAVITY_MASK) | g); }
    public void setIgnoreGravity(int id) { ignoreGravity = id; }
    protected ViewGroup.LayoutParams generateDefaultLayoutParams() { return new LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT); }
    public LayoutParams generateLayoutParams(AttributeSet attrs) { return new LayoutParams(getContext(), attrs); }
    protected ViewGroup.LayoutParams generateLayoutParams(ViewGroup.LayoutParams p) {
        if (p instanceof LayoutParams) return new LayoutParams((LayoutParams) p);
        if (p instanceof MarginLayoutParams) return new LayoutParams((MarginLayoutParams) p);
        return new LayoutParams(p);
    }
    protected boolean checkLayoutParams(ViewGroup.LayoutParams p) { return p instanceof LayoutParams; }

    private static final int[] HORIZONTAL_DEPS = { LEFT_OF, RIGHT_OF, ALIGN_LEFT, ALIGN_RIGHT };
    private static final int[] VERTICAL_DEPS = { ABOVE, BELOW, ALIGN_TOP, ALIGN_BOTTOM };

    /** Children ordered so that every anchor comes before its dependents. */
    private ArrayList<View> sorted(int[] verbs) {
        int n = getChildCount();
        ArrayList<View> out = new ArrayList<View>(n);
        boolean[] done = new boolean[n];
        for (int pass = 0; pass <= n && out.size() < n; pass++) {
            boolean progress = false;
            for (int i = 0; i < n; i++) {
                if (done[i]) continue;
                int[] r = ((LayoutParams) getChildAt(i).getLayoutParams()).resolved();
                boolean ready = true;
                for (int v : verbs) {
                    int id = r[v];
                    if (id == 0 || id == TRUE) continue;
                    for (int j = 0; j < n; j++) if (!done[j] && j != i && getChildAt(j).getId() == id) { ready = false; break; }
                    if (!ready) break;
                }
                if (ready) { done[i] = true; out.add(getChildAt(i)); progress = true; }
            }
            if (!progress) { for (int i = 0; i < n; i++) if (!done[i]) { done[i] = true; out.add(getChildAt(i)); } }
        }
        return out;
    }

    private View related(int[] rules, int verb) {
        int id = rules[verb];
        for (int guard = 0; guard < 32 && id != 0 && id != TRUE; guard++) {
            View v = null;
            for (int i = 0; i < getChildCount(); i++) if (getChildAt(i).getId() == id) { v = getChildAt(i); break; }
            if (v == null) return null;
            if (v.getVisibility() != GONE) return v;
            id = ((LayoutParams) v.getLayoutParams()).resolved()[verb];
        }
        return null;
    }
    private LayoutParams relatedParams(int[] rules, int verb) {
        View v = related(rules, verb);
        return v == null ? null : (LayoutParams) v.getLayoutParams();
    }

    protected void onMeasure(int ws, int hs) {
        int widthMode = MeasureSpec.getMode(ws), heightMode = MeasureSpec.getMode(hs);
        int widthSize = MeasureSpec.getSize(ws), heightSize = MeasureSpec.getSize(hs);
        int myWidth = widthMode == MeasureSpec.UNSPECIFIED ? -1 : widthSize;
        int myHeight = heightMode == MeasureSpec.UNSPECIFIED ? -1 : heightSize;
        int width = 0, height = 0;
        if (widthMode == MeasureSpec.EXACTLY) width = myWidth;
        if (heightMode == MeasureSpec.EXACTLY) height = myHeight;
        boolean wrapW = widthMode != MeasureSpec.EXACTLY, wrapH = heightMode != MeasureSpec.EXACTLY;
        boolean offsetH = false, offsetV = false;

        for (View child : sorted(HORIZONTAL_DEPS)) {
            if (child.getVisibility() == GONE) continue;
            LayoutParams lp = (LayoutParams) child.getLayoutParams();
            int[] rules = lp.resolved();
            applyHorizontalRules(lp, myWidth, rules);
            int cws = childSpec(lp.mLeft, lp.mRight, lp.width, lp.leftMargin, lp.rightMargin, getPaddingLeft(), getPaddingRight(), myWidth);
            int chs;
            if (myHeight < 0) chs = lp.height >= 0 ? MeasureSpec.makeMeasureSpec(lp.height, MeasureSpec.EXACTLY) : MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED);
            else {
                int max = Math.max(0, myHeight - getPaddingTop() - getPaddingBottom() - lp.topMargin - lp.bottomMargin);
                chs = lp.height >= 0 ? MeasureSpec.makeMeasureSpec(lp.height, MeasureSpec.EXACTLY)
                    : MeasureSpec.makeMeasureSpec(max, lp.height == LayoutParams.MATCH_PARENT ? MeasureSpec.EXACTLY : MeasureSpec.AT_MOST);
            }
            child.measure(cws, chs);
            if (positionHorizontal(child, lp, myWidth, wrapW, rules)) offsetH = true;
        }
        for (View child : sorted(VERTICAL_DEPS)) {
            if (child.getVisibility() == GONE) continue;
            LayoutParams lp = (LayoutParams) child.getLayoutParams();
            int[] rules = lp.resolved();
            applyVerticalRules(lp, myHeight, rules);
            int cws = childSpec(lp.mLeft, lp.mRight, lp.width, lp.leftMargin, lp.rightMargin, getPaddingLeft(), getPaddingRight(), myWidth);
            int chs = childSpec(lp.mTop, lp.mBottom, lp.height, lp.topMargin, lp.bottomMargin, getPaddingTop(), getPaddingBottom(), myHeight);
            child.measure(cws, chs);
            if (positionVertical(child, lp, myHeight, wrapH, rules)) offsetV = true;
            if (wrapW) width = Math.max(width, lp.mRight + lp.rightMargin);
            if (wrapH) height = Math.max(height, lp.mBottom + lp.bottomMargin);
        }
        if (wrapW) {
            width += getPaddingRight();
            if (getLayoutParams() != null && getLayoutParams().width >= 0) width = Math.max(width, getLayoutParams().width);
            width = resolveSize(Math.max(width, getSuggestedMinimumWidth()), ws);
            if (offsetH) {
                for (int i = 0; i < getChildCount(); i++) {
                    View child = getChildAt(i);
                    if (child.getVisibility() == GONE) continue;
                    LayoutParams lp = (LayoutParams) child.getLayoutParams();
                    int[] rules = lp.resolved();
                    if (rules[CENTER_IN_PARENT] != 0 || rules[CENTER_HORIZONTAL] != 0) centerHorizontal(child, lp, width);
                    else if (rules[ALIGN_PARENT_RIGHT] != 0) { int cw = child.getMeasuredWidth(); lp.mLeft = width - getPaddingRight() - cw - lp.rightMargin; lp.mRight = lp.mLeft + cw; }
                }
            }
        }
        if (wrapH) {
            height += getPaddingBottom();
            if (getLayoutParams() != null && getLayoutParams().height >= 0) height = Math.max(height, getLayoutParams().height);
            height = resolveSize(Math.max(height, getSuggestedMinimumHeight()), hs);
            if (offsetV) {
                for (int i = 0; i < getChildCount(); i++) {
                    View child = getChildAt(i);
                    if (child.getVisibility() == GONE) continue;
                    LayoutParams lp = (LayoutParams) child.getLayoutParams();
                    int[] rules = lp.resolved();
                    if (rules[CENTER_IN_PARENT] != 0 || rules[CENTER_VERTICAL] != 0) centerVertical(child, lp, height);
                    else if (rules[ALIGN_PARENT_BOTTOM] != 0) { int ch = child.getMeasuredHeight(); lp.mTop = height - getPaddingBottom() - ch - lp.bottomMargin; lp.mBottom = lp.mTop + ch; }
                }
            }
        }
        // Gravity applies to the bounding box of children not anchored to the parent.
        int hg = Gravity.getAbsoluteGravity(gravity, 0) & Gravity.HORIZONTAL_GRAVITY_MASK, vg = gravity & Gravity.VERTICAL_GRAVITY_MASK;
        if ((hg != Gravity.LEFT && hg != 0) || (vg != Gravity.TOP && vg != 0)) {
            int l = Integer.MAX_VALUE, t = Integer.MAX_VALUE, r = Integer.MIN_VALUE, b = Integer.MIN_VALUE;
            for (int i = 0; i < getChildCount(); i++) {
                View c = getChildAt(i);
                if (c.getVisibility() == GONE || c.getId() == ignoreGravity) continue;
                LayoutParams lp = (LayoutParams) c.getLayoutParams();
                l = Math.min(l, lp.mLeft - lp.leftMargin); t = Math.min(t, lp.mTop - lp.topMargin);
                r = Math.max(r, lp.mRight + lp.rightMargin); b = Math.max(b, lp.mBottom + lp.bottomMargin);
            }
            if (l != Integer.MAX_VALUE) {
                int availW = width - getPaddingLeft() - getPaddingRight(), availH = height - getPaddingTop() - getPaddingBottom();
                int dx = hg == Gravity.CENTER_HORIZONTAL ? getPaddingLeft() + (availW - (r - l)) / 2 - l : hg == Gravity.RIGHT ? width - getPaddingRight() - r : 0;
                int dy = vg == Gravity.CENTER_VERTICAL ? getPaddingTop() + (availH - (b - t)) / 2 - t : vg == Gravity.BOTTOM ? height - getPaddingBottom() - b : 0;
                for (int i = 0; i < getChildCount(); i++) {
                    View c = getChildAt(i);
                    if (c.getVisibility() == GONE || c.getId() == ignoreGravity) continue;
                    LayoutParams lp = (LayoutParams) c.getLayoutParams();
                    lp.mLeft += dx; lp.mRight += dx; lp.mTop += dy; lp.mBottom += dy;
                }
            }
        }
        setMeasuredDimension(width, height);
    }

    private void applyHorizontalRules(LayoutParams lp, int myWidth, int[] rules) {
        lp.mLeft = NOT_SET; lp.mRight = NOT_SET;
        LayoutParams a = relatedParams(rules, LEFT_OF);
        if (a != null) lp.mRight = a.mLeft - (a.leftMargin + lp.rightMargin);
        else if (lp.alignWithParent && rules[LEFT_OF] != 0 && myWidth >= 0) lp.mRight = myWidth - getPaddingRight() - lp.rightMargin;
        a = relatedParams(rules, RIGHT_OF);
        if (a != null) lp.mLeft = a.mRight + (a.rightMargin + lp.leftMargin);
        else if (lp.alignWithParent && rules[RIGHT_OF] != 0) lp.mLeft = getPaddingLeft() + lp.leftMargin;
        a = relatedParams(rules, ALIGN_LEFT);
        if (a != null) lp.mLeft = a.mLeft + lp.leftMargin;
        else if (lp.alignWithParent && rules[ALIGN_LEFT] != 0) lp.mLeft = getPaddingLeft() + lp.leftMargin;
        a = relatedParams(rules, ALIGN_RIGHT);
        if (a != null) lp.mRight = a.mRight - lp.rightMargin;
        else if (lp.alignWithParent && rules[ALIGN_RIGHT] != 0 && myWidth >= 0) lp.mRight = myWidth - getPaddingRight() - lp.rightMargin;
        if (rules[ALIGN_PARENT_LEFT] != 0) lp.mLeft = getPaddingLeft() + lp.leftMargin;
        if (rules[ALIGN_PARENT_RIGHT] != 0 && myWidth >= 0) lp.mRight = myWidth - getPaddingRight() - lp.rightMargin;
    }

    private void applyVerticalRules(LayoutParams lp, int myHeight, int[] rules) {
        lp.mTop = NOT_SET; lp.mBottom = NOT_SET;
        LayoutParams a = relatedParams(rules, ABOVE);
        if (a != null) lp.mBottom = a.mTop - (a.topMargin + lp.bottomMargin);
        else if (lp.alignWithParent && rules[ABOVE] != 0 && myHeight >= 0) lp.mBottom = myHeight - getPaddingBottom() - lp.bottomMargin;
        a = relatedParams(rules, BELOW);
        if (a != null) lp.mTop = a.mBottom + (a.bottomMargin + lp.topMargin);
        else if (lp.alignWithParent && rules[BELOW] != 0) lp.mTop = getPaddingTop() + lp.topMargin;
        a = relatedParams(rules, ALIGN_TOP);
        if (a != null) lp.mTop = a.mTop + lp.topMargin;
        else if (lp.alignWithParent && rules[ALIGN_TOP] != 0) lp.mTop = getPaddingTop() + lp.topMargin;
        a = relatedParams(rules, ALIGN_BOTTOM);
        if (a != null) lp.mBottom = a.mBottom - lp.bottomMargin;
        else if (lp.alignWithParent && rules[ALIGN_BOTTOM] != 0 && myHeight >= 0) lp.mBottom = myHeight - getPaddingBottom() - lp.bottomMargin;
        if (rules[ALIGN_PARENT_TOP] != 0) lp.mTop = getPaddingTop() + lp.topMargin;
        if (rules[ALIGN_PARENT_BOTTOM] != 0 && myHeight >= 0) lp.mBottom = myHeight - getPaddingBottom() - lp.bottomMargin;
    }

    private static int childSpec(int start, int end, int size, int startMargin, int endMargin, int startPad, int endPad, int mySize) {
        if (mySize < 0) {
            if (start != NOT_SET && end != NOT_SET) return MeasureSpec.makeMeasureSpec(Math.max(0, end - start), MeasureSpec.EXACTLY);
            if (size >= 0) return MeasureSpec.makeMeasureSpec(size, MeasureSpec.EXACTLY);
            return MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED);
        }
        int s = start == NOT_SET ? startPad + startMargin : start;
        int e = end == NOT_SET ? mySize - endPad - endMargin : end;
        int maxAvailable = e - s;
        if (start != NOT_SET && end != NOT_SET) return MeasureSpec.makeMeasureSpec(Math.max(0, maxAvailable), MeasureSpec.EXACTLY);
        if (size >= 0) return MeasureSpec.makeMeasureSpec(maxAvailable >= 0 ? Math.min(maxAvailable, size) : size, MeasureSpec.EXACTLY);
        if (size == LayoutParams.MATCH_PARENT) return MeasureSpec.makeMeasureSpec(Math.max(0, maxAvailable), MeasureSpec.EXACTLY);
        return maxAvailable >= 0 ? MeasureSpec.makeMeasureSpec(maxAvailable, MeasureSpec.AT_MOST) : MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED);
    }

    private boolean positionHorizontal(View child, LayoutParams lp, int myWidth, boolean wrap, int[] rules) {
        int w = child.getMeasuredWidth();
        if (lp.mLeft == NOT_SET && lp.mRight != NOT_SET) lp.mLeft = lp.mRight - w;
        else if (lp.mLeft != NOT_SET && lp.mRight == NOT_SET) lp.mRight = lp.mLeft + w;
        else if (lp.mLeft == NOT_SET) {
            if (rules[CENTER_IN_PARENT] != 0 || rules[CENTER_HORIZONTAL] != 0) {
                if (!wrap) centerHorizontal(child, lp, myWidth);
                else { lp.mLeft = getPaddingLeft() + lp.leftMargin; lp.mRight = lp.mLeft + w; }
                return true;
            }
            lp.mLeft = getPaddingLeft() + lp.leftMargin;
            lp.mRight = lp.mLeft + w;
        }
        return rules[ALIGN_PARENT_RIGHT] != 0;
    }

    private boolean positionVertical(View child, LayoutParams lp, int myHeight, boolean wrap, int[] rules) {
        int h = child.getMeasuredHeight();
        if (lp.mTop == NOT_SET && lp.mBottom != NOT_SET) lp.mTop = lp.mBottom - h;
        else if (lp.mTop != NOT_SET && lp.mBottom == NOT_SET) lp.mBottom = lp.mTop + h;
        else if (lp.mTop == NOT_SET) {
            if (rules[CENTER_IN_PARENT] != 0 || rules[CENTER_VERTICAL] != 0) {
                if (!wrap) centerVertical(child, lp, myHeight);
                else { lp.mTop = getPaddingTop() + lp.topMargin; lp.mBottom = lp.mTop + h; }
                return true;
            }
            lp.mTop = getPaddingTop() + lp.topMargin;
            lp.mBottom = lp.mTop + h;
        }
        return rules[ALIGN_PARENT_BOTTOM] != 0;
    }

    private static void centerHorizontal(View child, LayoutParams lp, int myWidth) {
        int w = child.getMeasuredWidth();
        int left = (myWidth - w) / 2;
        lp.mLeft = left; lp.mRight = left + w;
    }
    private static void centerVertical(View child, LayoutParams lp, int myHeight) {
        int h = child.getMeasuredHeight();
        int top = (myHeight - h) / 2;
        lp.mTop = top; lp.mBottom = top + h;
    }

    protected void onLayout(boolean changed, int l, int t, int r, int b) {
        for (int i = 0; i < getChildCount(); i++) {
            View child = getChildAt(i);
            if (child.getVisibility() == GONE) continue;
            LayoutParams lp = (LayoutParams) child.getLayoutParams();
            child.layout(lp.mLeft, lp.mTop, lp.mRight, lp.mBottom);
        }
    }
    protected String indroidType() { return "RelativeLayout"; }
}
