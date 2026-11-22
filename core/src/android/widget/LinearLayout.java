package android.widget;

import android.content.Context;
import android.util.AttributeSet;
import android.view.Gravity;
import android.view.Json;
import android.view.View;
import android.view.ViewGroup;
import android.view.XmlAttrs;

public class LinearLayout extends ViewGroup {
    public static final int HORIZONTAL = 0, VERTICAL = 1;
    public static final int SHOW_DIVIDER_NONE = 0, SHOW_DIVIDER_BEGINNING = 1, SHOW_DIVIDER_MIDDLE = 2, SHOW_DIVIDER_END = 4;
    public static class LayoutParams extends ViewGroup.MarginLayoutParams {
        public float weight;
        public int gravity = -1;
        public LayoutParams(int w, int h) { super(w, h); }
        public LayoutParams(int w, int h, float weight) { super(w, h); this.weight = weight; }
        public LayoutParams(ViewGroup.LayoutParams p) { super(p); }
        public LayoutParams(ViewGroup.MarginLayoutParams p) { super(p); }
        public LayoutParams(LayoutParams p) { super(p); weight = p.weight; gravity = p.gravity; }
        public LayoutParams(Context c, AttributeSet attrs) {
            super(c, attrs);
            XmlAttrs a = attrs instanceof XmlAttrs ? (XmlAttrs) attrs : XmlAttrs.EMPTY;
            weight = a.dimension("layout_weight", 0);
            String g = a.raw("layout_gravity");
            if (g != null) gravity = a.integer("layout_gravity", -1);
        }
    }
    private int orientation = HORIZONTAL;
    private int gravity = Gravity.START | Gravity.TOP;
    private float weightSum = -1;
    private int totalLength;
    private boolean useLargestChild;

    public LinearLayout(Context c) { super(c); }
    public LinearLayout(Context c, AttributeSet a) { this(c, a, 0); }
    public LinearLayout(Context c, AttributeSet a, int s) { this(c, a, s, 0); }
    public LinearLayout(Context c, AttributeSet a, int s, int r) {
        super(c, a, s, r);
        if (a instanceof XmlAttrs) {
            XmlAttrs x = (XmlAttrs) a;
            String o = x.string("orientation");
            orientation = o != null && (o.equals("vertical") || o.equals("1")) ? VERTICAL : HORIZONTAL;
            int g = x.integer("gravity", 0);
            if (g != 0) setGravity(g);
            weightSum = x.dimension("weightSum", -1);
            useLargestChild = x.bool("measureWithLargestChild", false);
        }
    }
    public void setOrientation(int o) { if (o != orientation) { orientation = o; requestLayout(); } }
    public int getOrientation() { return orientation; }
    public void setGravity(int g) {
        if ((g & Gravity.RELATIVE_HORIZONTAL_GRAVITY_MASK) == 0) g |= Gravity.START;
        if ((g & Gravity.VERTICAL_GRAVITY_MASK) == 0) g |= Gravity.TOP;
        if (g != gravity) { gravity = g; requestLayout(); }
    }
    public int getGravity() { return gravity; }
    public void setHorizontalGravity(int h) { setGravity((gravity & ~Gravity.RELATIVE_HORIZONTAL_GRAVITY_MASK) | (h & Gravity.RELATIVE_HORIZONTAL_GRAVITY_MASK)); }
    public void setVerticalGravity(int v) { setGravity((gravity & ~Gravity.VERTICAL_GRAVITY_MASK) | (v & Gravity.VERTICAL_GRAVITY_MASK)); }
    public void setWeightSum(float w) { weightSum = w; requestLayout(); }
    public float getWeightSum() { return weightSum; }
    public void setBaselineAligned(boolean b) {}
    public boolean isBaselineAligned() { return false; }
    public void setMeasureWithLargestChildEnabled(boolean b) { useLargestChild = b; requestLayout(); }
    public boolean isMeasureWithLargestChildEnabled() { return useLargestChild; }
    public void setShowDividers(int s) {}
    public int getShowDividers() { return 0; }
    public void setDividerDrawable(android.graphics.drawable.Drawable d) {}
    public android.graphics.drawable.Drawable getDividerDrawable() { return null; }
    public void setDividerPadding(int p) {}
    protected ViewGroup.LayoutParams generateDefaultLayoutParams() {
        return orientation == HORIZONTAL ? new LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT) : new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT);
    }
    public LayoutParams generateLayoutParams(AttributeSet attrs) { return new LayoutParams(getContext(), attrs); }
    protected ViewGroup.LayoutParams generateLayoutParams(ViewGroup.LayoutParams p) {
        if (p instanceof LayoutParams) return new LayoutParams((LayoutParams) p);
        if (p instanceof MarginLayoutParams) return new LayoutParams((MarginLayoutParams) p);
        return new LayoutParams(p);
    }
    protected boolean checkLayoutParams(ViewGroup.LayoutParams p) { return p instanceof LayoutParams; }

    protected void onMeasure(int ws, int hs) {
        if (orientation == VERTICAL) measureVertical(ws, hs); else measureHorizontal(ws, hs);
    }

    void measureVertical(int ws, int hs) {
        totalLength = 0;
        int maxWidth = 0, childState = 0, alternativeMaxWidth = 0, weightedMaxWidth = 0;
        boolean allFillParent = true, matchWidth = false, skippedMeasure = false;
        float totalWeight = 0;
        int count = getChildCount();
        int widthMode = MeasureSpec.getMode(ws), heightMode = MeasureSpec.getMode(hs);
        int largestChildHeight = Integer.MIN_VALUE, consumedExcessSpace = 0;
        for (int i = 0; i < count; i++) {
            View child = getChildAt(i);
            if (child == null || child.getVisibility() == GONE) continue;
            LayoutParams lp = (LayoutParams) child.getLayoutParams();
            totalWeight += lp.weight;
            boolean useExcessSpace = lp.height == 0 && lp.weight > 0;
            if (heightMode == MeasureSpec.EXACTLY && useExcessSpace) {
                int tl = totalLength;
                totalLength = Math.max(tl, tl + lp.topMargin + lp.bottomMargin);
                skippedMeasure = true;
            } else {
                if (useExcessSpace) lp.height = LayoutParams.WRAP_CONTENT;
                int usedHeight = totalWeight == 0 ? totalLength : 0;
                measureChildWithMargins(child, ws, 0, hs, usedHeight);
                int childHeight = child.getMeasuredHeight();
                if (useExcessSpace) { lp.height = 0; consumedExcessSpace += childHeight; }
                int tl = totalLength;
                totalLength = Math.max(tl, tl + childHeight + lp.topMargin + lp.bottomMargin);
                if (useLargestChild) largestChildHeight = Math.max(childHeight, largestChildHeight);
            }
            boolean matchWidthLocally = false;
            if (widthMode != MeasureSpec.EXACTLY && lp.width == LayoutParams.MATCH_PARENT) { matchWidth = true; matchWidthLocally = true; }
            int margin = lp.leftMargin + lp.rightMargin;
            int measuredWidth = child.getMeasuredWidth() + margin;
            maxWidth = Math.max(maxWidth, measuredWidth);
            childState = combineMeasuredStates(childState, child.getMeasuredState());
            allFillParent = allFillParent && lp.width == LayoutParams.MATCH_PARENT;
            if (lp.weight > 0) weightedMaxWidth = Math.max(weightedMaxWidth, matchWidthLocally ? margin : measuredWidth);
            else alternativeMaxWidth = Math.max(alternativeMaxWidth, matchWidthLocally ? margin : measuredWidth);
        }
        if (useLargestChild && (heightMode == MeasureSpec.AT_MOST || heightMode == MeasureSpec.UNSPECIFIED)) {
            totalLength = 0;
            for (int i = 0; i < count; i++) {
                View child = getChildAt(i);
                if (child == null || child.getVisibility() == GONE) continue;
                LayoutParams lp = (LayoutParams) child.getLayoutParams();
                int tl = totalLength;
                totalLength = Math.max(tl, tl + largestChildHeight + lp.topMargin + lp.bottomMargin);
            }
        }
        totalLength += getPaddingTop() + getPaddingBottom();
        int heightSize = Math.max(totalLength, getSuggestedMinimumHeight());
        int heightSizeAndState = resolveSizeAndState(heightSize, hs, 0);
        heightSize = heightSizeAndState & MEASURED_SIZE_MASK;
        int remainingExcess = heightSize - totalLength + consumedExcessSpace;
        if (skippedMeasure || (remainingExcess != 0 && totalWeight > 0)) {
            float remainingWeightSum = weightSum > 0 ? weightSum : totalWeight;
            totalLength = 0;
            for (int i = 0; i < count; i++) {
                View child = getChildAt(i);
                if (child == null || child.getVisibility() == GONE) continue;
                LayoutParams lp = (LayoutParams) child.getLayoutParams();
                float childWeight = lp.weight;
                if (childWeight > 0) {
                    int share = (int) (childWeight * remainingExcess / remainingWeightSum);
                    remainingExcess -= share;
                    remainingWeightSum -= childWeight;
                    int childHeight;
                    if (useLargestChild && heightMode != MeasureSpec.EXACTLY) childHeight = largestChildHeight;
                    else if (lp.height == 0 && heightMode == MeasureSpec.EXACTLY) childHeight = share;
                    else childHeight = child.getMeasuredHeight() + share;
                    int chs = MeasureSpec.makeMeasureSpec(Math.max(0, childHeight), MeasureSpec.EXACTLY);
                    int cws = getChildMeasureSpec(ws, getPaddingLeft() + getPaddingRight() + lp.leftMargin + lp.rightMargin, lp.width);
                    child.measure(cws, chs);
                    childState = combineMeasuredStates(childState, child.getMeasuredState() & (MEASURED_STATE_MASK >> MEASURED_HEIGHT_STATE_SHIFT));
                }
                int margin = lp.leftMargin + lp.rightMargin;
                int measuredWidth = child.getMeasuredWidth() + margin;
                maxWidth = Math.max(maxWidth, measuredWidth);
                boolean matchWidthLocally = widthMode != MeasureSpec.EXACTLY && lp.width == LayoutParams.MATCH_PARENT;
                alternativeMaxWidth = Math.max(alternativeMaxWidth, matchWidthLocally ? margin : measuredWidth);
                allFillParent = allFillParent && lp.width == LayoutParams.MATCH_PARENT;
                int tl = totalLength;
                totalLength = Math.max(tl, tl + child.getMeasuredHeight() + lp.topMargin + lp.bottomMargin);
            }
            totalLength += getPaddingTop() + getPaddingBottom();
        } else {
            alternativeMaxWidth = Math.max(alternativeMaxWidth, weightedMaxWidth);
        }
        if (!allFillParent && widthMode != MeasureSpec.EXACTLY) maxWidth = alternativeMaxWidth;
        maxWidth += getPaddingLeft() + getPaddingRight();
        maxWidth = Math.max(maxWidth, getSuggestedMinimumWidth());
        setMeasuredDimension(resolveSizeAndState(maxWidth, ws, childState), heightSizeAndState);
        if (matchWidth) forceUniformWidth(count, hs);
    }

    private void forceUniformWidth(int count, int hs) {
        int uniform = MeasureSpec.makeMeasureSpec(getMeasuredWidth(), MeasureSpec.EXACTLY);
        for (int i = 0; i < count; i++) {
            View child = getChildAt(i);
            if (child == null || child.getVisibility() == GONE) continue;
            LayoutParams lp = (LayoutParams) child.getLayoutParams();
            if (lp.width == LayoutParams.MATCH_PARENT) {
                int old = lp.height;
                lp.height = child.getMeasuredHeight();
                measureChildWithMargins(child, uniform, 0, hs, 0);
                lp.height = old;
            }
        }
    }

    void measureHorizontal(int ws, int hs) {
        totalLength = 0;
        int maxHeight = 0, childState = 0, alternativeMaxHeight = 0, weightedMaxHeight = 0;
        boolean allFillParent = true, matchHeight = false, skippedMeasure = false;
        float totalWeight = 0;
        int count = getChildCount();
        int widthMode = MeasureSpec.getMode(ws), heightMode = MeasureSpec.getMode(hs);
        int largestChildWidth = Integer.MIN_VALUE, usedExcessSpace = 0;
        boolean isExactly = widthMode == MeasureSpec.EXACTLY;
        for (int i = 0; i < count; i++) {
            View child = getChildAt(i);
            if (child == null || child.getVisibility() == GONE) continue;
            LayoutParams lp = (LayoutParams) child.getLayoutParams();
            totalWeight += lp.weight;
            boolean useExcessSpace = lp.width == 0 && lp.weight > 0;
            if (widthMode == MeasureSpec.EXACTLY && useExcessSpace) {
                if (isExactly) totalLength += lp.leftMargin + lp.rightMargin;
                else { int tl = totalLength; totalLength = Math.max(tl, tl + lp.leftMargin + lp.rightMargin); }
                skippedMeasure = true;
            } else {
                if (useExcessSpace) lp.width = LayoutParams.WRAP_CONTENT;
                int usedWidth = totalWeight == 0 ? totalLength : 0;
                measureChildWithMargins(child, ws, usedWidth, hs, 0);
                int childWidth = child.getMeasuredWidth();
                if (useExcessSpace) { lp.width = 0; usedExcessSpace += childWidth; }
                if (isExactly) totalLength += childWidth + lp.leftMargin + lp.rightMargin;
                else { int tl = totalLength; totalLength = Math.max(tl, tl + childWidth + lp.leftMargin + lp.rightMargin); }
                if (useLargestChild) largestChildWidth = Math.max(childWidth, largestChildWidth);
            }
            boolean matchHeightLocally = false;
            if (heightMode != MeasureSpec.EXACTLY && lp.height == LayoutParams.MATCH_PARENT) { matchHeight = true; matchHeightLocally = true; }
            int margin = lp.topMargin + lp.bottomMargin;
            int childHeight = child.getMeasuredHeight() + margin;
            childState = combineMeasuredStates(childState, child.getMeasuredState());
            maxHeight = Math.max(maxHeight, childHeight);
            allFillParent = allFillParent && lp.height == LayoutParams.MATCH_PARENT;
            if (lp.weight > 0) weightedMaxHeight = Math.max(weightedMaxHeight, matchHeightLocally ? margin : childHeight);
            else alternativeMaxHeight = Math.max(alternativeMaxHeight, matchHeightLocally ? margin : childHeight);
        }
        if (useLargestChild && (widthMode == MeasureSpec.AT_MOST || widthMode == MeasureSpec.UNSPECIFIED)) {
            totalLength = 0;
            for (int i = 0; i < count; i++) {
                View child = getChildAt(i);
                if (child == null || child.getVisibility() == GONE) continue;
                LayoutParams lp = (LayoutParams) child.getLayoutParams();
                totalLength += largestChildWidth + lp.leftMargin + lp.rightMargin;
            }
        }
        totalLength += getPaddingLeft() + getPaddingRight();
        int widthSize = Math.max(totalLength, getSuggestedMinimumWidth());
        int widthSizeAndState = resolveSizeAndState(widthSize, ws, 0);
        widthSize = widthSizeAndState & MEASURED_SIZE_MASK;
        int remainingExcess = widthSize - totalLength + usedExcessSpace;
        if (skippedMeasure || (remainingExcess != 0 && totalWeight > 0)) {
            float remainingWeightSum = weightSum > 0 ? weightSum : totalWeight;
            maxHeight = -1;
            totalLength = 0;
            for (int i = 0; i < count; i++) {
                View child = getChildAt(i);
                if (child == null || child.getVisibility() == GONE) continue;
                LayoutParams lp = (LayoutParams) child.getLayoutParams();
                float childWeight = lp.weight;
                if (childWeight > 0) {
                    int share = (int) (childWeight * remainingExcess / remainingWeightSum);
                    remainingExcess -= share;
                    remainingWeightSum -= childWeight;
                    int childWidth;
                    if (useLargestChild && widthMode != MeasureSpec.EXACTLY) childWidth = largestChildWidth;
                    else if (lp.width == 0 && widthMode == MeasureSpec.EXACTLY) childWidth = share;
                    else childWidth = child.getMeasuredWidth() + share;
                    int cws = MeasureSpec.makeMeasureSpec(Math.max(0, childWidth), MeasureSpec.EXACTLY);
                    int chs = getChildMeasureSpec(hs, getPaddingTop() + getPaddingBottom() + lp.topMargin + lp.bottomMargin, lp.height);
                    child.measure(cws, chs);
                    childState = combineMeasuredStates(childState, child.getMeasuredState() & MEASURED_STATE_MASK);
                }
                if (isExactly) totalLength += child.getMeasuredWidth() + lp.leftMargin + lp.rightMargin;
                else { int tl = totalLength; totalLength = Math.max(tl, tl + child.getMeasuredWidth() + lp.leftMargin + lp.rightMargin); }
                boolean matchHeightLocally = heightMode != MeasureSpec.EXACTLY && lp.height == LayoutParams.MATCH_PARENT;
                int margin = lp.topMargin + lp.bottomMargin;
                int childHeight = child.getMeasuredHeight() + margin;
                maxHeight = Math.max(maxHeight, childHeight);
                alternativeMaxHeight = Math.max(alternativeMaxHeight, matchHeightLocally ? margin : childHeight);
                allFillParent = allFillParent && lp.height == LayoutParams.MATCH_PARENT;
            }
            totalLength += getPaddingLeft() + getPaddingRight();
        } else {
            alternativeMaxHeight = Math.max(alternativeMaxHeight, weightedMaxHeight);
        }
        if (!allFillParent && heightMode != MeasureSpec.EXACTLY) maxHeight = alternativeMaxHeight;
        maxHeight += getPaddingTop() + getPaddingBottom();
        maxHeight = Math.max(maxHeight, getSuggestedMinimumHeight());
        setMeasuredDimension(widthSizeAndState | (childState & MEASURED_STATE_MASK), resolveSizeAndState(maxHeight, hs, childState << MEASURED_HEIGHT_STATE_SHIFT));
        if (matchHeight) forceUniformHeight(count, ws);
    }

    private void forceUniformHeight(int count, int ws) {
        int uniform = MeasureSpec.makeMeasureSpec(getMeasuredHeight(), MeasureSpec.EXACTLY);
        for (int i = 0; i < count; i++) {
            View child = getChildAt(i);
            if (child == null || child.getVisibility() == GONE) continue;
            LayoutParams lp = (LayoutParams) child.getLayoutParams();
            if (lp.height == LayoutParams.MATCH_PARENT) {
                int old = lp.width;
                lp.width = child.getMeasuredWidth();
                measureChildWithMargins(child, ws, 0, uniform, 0);
                lp.width = old;
            }
        }
    }

    protected void onLayout(boolean changed, int l, int t, int r, int b) {
        if (orientation == VERTICAL) layoutVertical(l, t, r, b); else layoutHorizontal(l, t, r, b);
    }

    void layoutVertical(int left, int top, int right, int bottom) {
        int paddingLeft = getPaddingLeft();
        int width = right - left;
        int childRight = width - getPaddingRight();
        int childSpace = width - paddingLeft - getPaddingRight();
        int majorGravity = gravity & Gravity.VERTICAL_GRAVITY_MASK;
        int minorGravity = gravity & Gravity.RELATIVE_HORIZONTAL_GRAVITY_MASK;
        int childTop;
        switch (majorGravity) {
            case Gravity.BOTTOM: childTop = getPaddingTop() + bottom - top - totalLength; break;
            case Gravity.CENTER_VERTICAL: childTop = getPaddingTop() + (bottom - top - totalLength) / 2; break;
            default: childTop = getPaddingTop();
        }
        for (int i = 0; i < getChildCount(); i++) {
            View child = getChildAt(i);
            if (child == null || child.getVisibility() == GONE) continue;
            int cw = child.getMeasuredWidth(), ch = child.getMeasuredHeight();
            LayoutParams lp = (LayoutParams) child.getLayoutParams();
            int g = lp.gravity < 0 ? minorGravity : lp.gravity;
            int childLeft;
            switch (Gravity.getAbsoluteGravity(g, 0) & Gravity.HORIZONTAL_GRAVITY_MASK) {
                case Gravity.CENTER_HORIZONTAL: childLeft = paddingLeft + (childSpace - cw) / 2 + lp.leftMargin - lp.rightMargin; break;
                case Gravity.RIGHT: childLeft = childRight - cw - lp.rightMargin; break;
                default: childLeft = paddingLeft + lp.leftMargin;
            }
            childTop += lp.topMargin;
            child.layout(childLeft, childTop, childLeft + cw, childTop + ch);
            childTop += ch + lp.bottomMargin;
        }
    }

    void layoutHorizontal(int left, int top, int right, int bottom) {
        int paddingTop = getPaddingTop();
        int height = bottom - top;
        int childBottom = height - getPaddingBottom();
        int childSpace = height - paddingTop - getPaddingBottom();
        int majorGravity = Gravity.getAbsoluteGravity(gravity & Gravity.RELATIVE_HORIZONTAL_GRAVITY_MASK, 0) & Gravity.HORIZONTAL_GRAVITY_MASK;
        int minorGravity = gravity & Gravity.VERTICAL_GRAVITY_MASK;
        int childLeft;
        switch (majorGravity) {
            case Gravity.RIGHT: childLeft = getPaddingLeft() + right - left - totalLength; break;
            case Gravity.CENTER_HORIZONTAL: childLeft = getPaddingLeft() + (right - left - totalLength) / 2; break;
            default: childLeft = getPaddingLeft();
        }
        for (int i = 0; i < getChildCount(); i++) {
            View child = getChildAt(i);
            if (child == null || child.getVisibility() == GONE) continue;
            int cw = child.getMeasuredWidth(), ch = child.getMeasuredHeight();
            LayoutParams lp = (LayoutParams) child.getLayoutParams();
            int g = lp.gravity < 0 ? minorGravity : lp.gravity;
            int childTop;
            switch (g & Gravity.VERTICAL_GRAVITY_MASK) {
                case Gravity.CENTER_VERTICAL: childTop = paddingTop + (childSpace - ch) / 2 + lp.topMargin - lp.bottomMargin; break;
                case Gravity.BOTTOM: childTop = childBottom - ch - lp.bottomMargin; break;
                default: childTop = paddingTop + lp.topMargin;
            }
            childLeft += lp.leftMargin;
            child.layout(childLeft, childTop, childLeft + cw, childTop + ch);
            childLeft += cw + lp.rightMargin;
        }
    }

    protected String indroidType() { return "LinearLayout"; }
    protected void indroidProps(StringBuilder sb) {
        Json.key(sb, "orientation"); Json.str(sb, orientation == VERTICAL ? "vertical" : "horizontal");
        String g = Gravity.describe(gravity == (Gravity.START | Gravity.TOP) ? 0 : gravity);
        if (g != null) { Json.key(sb, "gravity"); Json.str(sb, g); }
        super.indroidProps(sb);
    }
 public void onRtlPropertiesChanged(int direction) {}
}
