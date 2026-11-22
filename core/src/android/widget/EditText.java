package android.widget;
import android.content.Context;
import android.text.Editable;
import android.text.SpannableStringBuilder;
import android.util.AttributeSet;
public class EditText extends TextView {
    private SpannableStringBuilder editable;
    public EditText(Context c) { this(c, null); }
    public EditText(Context c, AttributeSet a) { this(c, a, 16842862 /*editTextStyle*/); }
    public EditText(Context c, AttributeSet a, int s) { this(c, a, s, 0); }
    public EditText(Context c, AttributeSet a, int s, int r) { super(c, a, s, r); editDefaults(); }
    private void editDefaults() {
        if (getPaddingLeft() == 0 && getPaddingTop() == 0 && getPaddingRight() == 0 && getPaddingBottom() == 0) setPadding(4, 12, 4, 12);
        if (getMinimumHeight() == 0) setMinimumHeight(48);
        if (getGravity() == 0) setGravity(android.view.Gravity.START | android.view.Gravity.CENTER_VERTICAL);
    }
    void onSetText() { editable = null; }
    public Editable getText() {
        if (editable == null) {
            editable = new SpannableStringBuilder(text) {
                public SpannableStringBuilder replace(int st, int en, CharSequence t) { super.replace(st, en, t); EditText.this.text = toString(); return this; }
                public SpannableStringBuilder insert(int w, CharSequence t) { super.insert(w, t); EditText.this.text = toString(); return this; }
                public SpannableStringBuilder delete(int st, int en) { super.delete(st, en); EditText.this.text = toString(); return this; }
                public SpannableStringBuilder append(CharSequence t) { super.append(t); EditText.this.text = toString(); return this; }
                public SpannableStringBuilder append(char c) { super.append(c); EditText.this.text = toString(); return this; }
                public void clear() { super.clear(); EditText.this.text = ""; }
            };
        }
        return editable;
    }
    public void selectAll() {}
    public void setSelection(int start, int stop) {}
    public void extendSelection(int i) {}
    protected String indroidType() { return "EditText"; }
 public android.graphics.drawable.Drawable getTextCursorDrawable() { return null; } public boolean onTextContextMenuItem(int id) { return false; }
}
