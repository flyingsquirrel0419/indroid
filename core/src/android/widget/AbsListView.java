package android.widget;
import android.content.Context;
import android.util.AttributeSet;
import android.view.View;
public abstract class AbsListView extends AdapterView<ListAdapter> {
    public static final int CHOICE_MODE_NONE = 0, CHOICE_MODE_SINGLE = 1, CHOICE_MODE_MULTIPLE = 2;
    public interface OnScrollListener { int SCROLL_STATE_IDLE = 0; void onScrollStateChanged(AbsListView v, int state); void onScroll(AbsListView v, int first, int visible, int total); }
    ListAdapter adapter;
    public AbsListView(Context c) { super(c); }
    public AbsListView(Context c, AttributeSet a) { super(c, a); }
    public AbsListView(Context c, AttributeSet a, int s) { super(c, a, s); }
    public ListAdapter getAdapter() { return adapter; }
    public void setAdapter(ListAdapter a) {
        if (adapter != null) adapter.unregisterDataSetObserver(observer);
        adapter = a;
        if (a != null) a.registerDataSetObserver(observer);
        rebuild();
    }
    public void setOnScrollListener(OnScrollListener l) {}
    public void setChoiceMode(int m) { choiceMode=m; }
    public void setItemChecked(int p, boolean v) {}
    public void setTextFilterEnabled(boolean b) {}
    public void setFastScrollEnabled(boolean b) {}
    public void smoothScrollToPosition(int p) {}
    public void setSelector(int r) { selector=r==0?null:getResources().getDrawable(r); }
    public void setStackFromBottom(boolean b) {}
    public void setTranscriptMode(int m) {}
    public void invalidateViews() { rebuild(); }
    protected android.view.ViewGroup.LayoutParams generateDefaultLayoutParams() { return new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT); }

    /** Columns used by layout (GridView overrides). */
    int columns(int width) { return 1; }
    int hSpacing() { return 0; }
    int vSpacing() { return 0; }
    private int contentHeight;

    // All rows are materialized; the list scrolls natively on the host.
    protected void onMeasure(int ws, int hs) {
        int widthSize = MeasureSpec.getSize(ws), widthMode = MeasureSpec.getMode(ws);
        int inner = Math.max(0, widthSize - getPaddingLeft() - getPaddingRight());
        int cols = Math.max(1, columns(inner));
        int colW = cols == 1 ? inner : Math.max(0, (inner - hSpacing() * (cols - 1)) / cols);
        int maxW = 0, total = 0, rowH = 0;
        for (int i = 0; i < getChildCount(); i++) {
            View c = getChildAt(i);
            if (c.getVisibility() == GONE) continue;
            android.view.ViewGroup.LayoutParams lp = c.getLayoutParams();
            int cws = widthMode == MeasureSpec.UNSPECIFIED ? MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED) : MeasureSpec.makeMeasureSpec(colW, MeasureSpec.EXACTLY);
            int chs = lp.height >= 0 ? MeasureSpec.makeMeasureSpec(lp.height, MeasureSpec.EXACTLY) : MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED);
            c.measure(cws, chs);
            maxW = Math.max(maxW, c.getMeasuredWidth());
            rowH = Math.max(rowH, c.getMeasuredHeight());
            if (i % cols == cols - 1 || i == getChildCount() - 1) { total += rowH + (total > 0 ? vSpacing() : 0); rowH = 0; }
        }
        contentHeight = total;
        int w = widthMode == MeasureSpec.UNSPECIFIED ? maxW + getPaddingLeft() + getPaddingRight() : widthSize;
        int h = resolveSize(Math.max(total + getPaddingTop() + getPaddingBottom(), getSuggestedMinimumHeight()), hs);
        setMeasuredDimension(w, h);
    }
    protected void onLayout(boolean changed, int l, int t, int r, int b) {
        int inner = Math.max(0, r - l - getPaddingLeft() - getPaddingRight());
        int cols = Math.max(1, columns(inner));
        int colW = cols == 1 ? inner : Math.max(0, (inner - hSpacing() * (cols - 1)) / cols);
        int y = 0, rowH = 0, col = 0;
        for (int i = 0; i < getChildCount(); i++) {
            View c = getChildAt(i);
            if (c.getVisibility() == GONE) continue;
            int x = col * (colW + hSpacing());
            c.layout(x, y, x + c.getMeasuredWidth(), y + c.getMeasuredHeight());
            rowH = Math.max(rowH, c.getMeasuredHeight());
            if (++col == cols) { col = 0; y += rowH + vSpacing(); rowH = 0; }
        }
    }
    protected String indroidType() { return "ScrollView"; }
    protected void indroidProps(StringBuilder sb) {
        android.view.Json.key(sb, "children");
        int w = Math.max(0, getWidth() - getPaddingLeft() - getPaddingRight());
        sb.append("[{\"id\":0,\"type\":\"FrameLayout\",\"frame\":[").append(getPaddingLeft()).append(',').append(getPaddingTop()).append(',').append(w).append(',').append(contentHeight)
          .append("],\"orientation\":\"vertical\",\"width\":\"match\",\"height\":\"wrap\"");
        super.indroidProps(sb);
        sb.append("}]");
    }
    public interface SelectionBoundsAdjuster { void adjustListItemSelectionBounds(android.graphics.Rect rect); }
    private int choiceMode; private boolean childEnabled=true; private android.graphics.drawable.Drawable selector;
    public AbsListView(Context c,AttributeSet a,int s,int r) { super(c,a,s,r); }
    public int getChoiceMode() { return choiceMode; } public int getListPaddingTop() { return getPaddingTop(); } public int getListPaddingBottom() { return getPaddingBottom(); } public android.graphics.drawable.Drawable getSelector() { return selector; } public void setSelector(android.graphics.drawable.Drawable d) { selector=d; } public boolean isSelectedChildViewEnabled() { return childEnabled; } public void setSelectedChildViewEnabled(boolean b) { childEnabled=b; } public void setCacheColorHint(int c) {} protected void layoutChildren() {} public void setSelectionFromTop(int p,int y) { setSelection(p); } public void scrollListBy(int y) { scrollBy(0,y); } public int pointToPosition(int x,int y) { for(int i=0;i<getChildCount();i++) { View v=getChildAt(i); if(x>=v.getLeft() && x<v.getRight() && y>=v.getTop() && y<v.getBottom()) return i; } return INVALID_POSITION; }

}
