package android.widget;

import android.content.Context;
import android.util.AttributeSet;
import android.view.Json;
import android.view.View;

/** Rendered by the host as a menu button; item choice comes back as a click on a row. */
public class Spinner extends AbsSpinner {
    private SpinnerAdapter adapter;
    private int selected = -1;
    public Spinner(Context c) { super(c); }
    public Spinner(Context c, AttributeSet a) { this(c, a, 16842881 /*spinnerStyle*/); }
    public Spinner(Context c, AttributeSet a, int s) { super(c, a, s); }
    public SpinnerAdapter getAdapter() { return adapter; }
    public void setAdapter(SpinnerAdapter a) {
        if (adapter != null) adapter.unregisterDataSetObserver(observer);
        adapter = a;
        if (a != null) a.registerDataSetObserver(observer);
        rebuild();
        if (a != null && a.getCount() > 0 && selected < 0) setSelection(0);
    }
    public void setSelection(int p) {
        if (adapter == null || p < 0 || p >= adapter.getCount()) return;
        boolean changed = p != selected;
        selected = p;
        if (changed && itemSelected != null) itemSelected.onItemSelected(this, getChildAt(p), p, adapter.getItemId(p));
    }
    public void setSelection(int p, boolean animate) { setSelection(p); }
    public int getSelectedItemPosition() { return selected; }
    public Object getSelectedItem() { return selected < 0 || adapter == null ? null : adapter.getItem(selected); }
    public long getSelectedItemId() { return selected < 0 || adapter == null ? INVALID_ROW_ID : adapter.getItemId(selected); }
    public View getSelectedView() { return selected < 0 ? null : getChildAt(selected); }
    public void setPrompt(CharSequence p) { prompt=p; }
    void onRowClicked(View row, int pos) { setSelection(pos); }
    protected void onMeasure(int ws, int hs) {
        float w = 0;
        for (int i = 0; i < getChildCount(); i++) {
            View c = getChildAt(i);
            CharSequence t = c instanceof TextView ? ((TextView) c).getText() : "";
            w = Math.max(w, android.view.VMText.width(t.toString(), 16f, 0));
        }
        int width = resolveSize(Math.max(getSuggestedMinimumWidth(), (int) Math.ceil(w) + 48 + getPaddingLeft() + getPaddingRight()), ws);
        int height = resolveSize(Math.max(getSuggestedMinimumHeight(), 48), hs);
        setMeasuredDimension(width, height);
    }
    protected void onLayout(boolean changed, int l, int t, int r, int b) {}
    protected String indroidType() { return "Spinner"; }
    protected void indroidProps(StringBuilder sb) {
        Json.key(sb, "selected"); sb.append(selected);
        Json.key(sb, "items"); sb.append('[');
        for (int i = 0; i < getChildCount(); i++) {
            if (i > 0) sb.append(',');
            View c = getChildAt(i);
            sb.append("{\"id\":").append(c.indroidUid).append(",\"text\":");
            CharSequence t = c instanceof TextView ? ((TextView) c).getText() : String.valueOf(adapter.getItem(i));
            Json.str(sb, t);
            sb.append('}');
        }
        sb.append(']');
    }
    private int horizontalOffset,verticalOffset,dropDownWidth; private CharSequence prompt; private android.graphics.drawable.Drawable popupBackground;
    public Spinner(Context c,AttributeSet a,int s,int r) { super(c,a,s,r); }
    public void setDropDownHorizontalOffset(int v) { horizontalOffset=v; } public int getDropDownHorizontalOffset() { return horizontalOffset; } public void setDropDownVerticalOffset(int v) { verticalOffset=v; } public int getDropDownVerticalOffset() { return verticalOffset; } public void setDropDownWidth(int v) { dropDownWidth=v; } public int getDropDownWidth() { return dropDownWidth; } public void setPopupBackgroundDrawable(android.graphics.drawable.Drawable d) { popupBackground=d; } public android.graphics.drawable.Drawable getPopupBackground() { return popupBackground; } public CharSequence getPrompt() { return prompt; }

 protected android.os.Parcelable onSaveInstanceState() { return null; } protected void onRestoreInstanceState(android.os.Parcelable state) {}
}
