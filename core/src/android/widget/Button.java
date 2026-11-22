package android.widget;
import android.content.Context;
import android.util.AttributeSet;
public class Button extends TextView {
    public Button(Context c) { this(c, null); }
    public Button(Context c, AttributeSet a) { this(c, a, 16842824 /*buttonStyle*/); }
    public Button(Context c, AttributeSet a, int s) { this(c, a, s, 0); }
    public Button(Context c, AttributeSet a, int s, int r) { super(c, a, s, r); buttonDefaults(); }
    /** Widget.Material.Button metrics: 48dp touch height, 88dp min width, centered text. */
    void buttonDefaults() {
        if (getPaddingLeft() == 0 && getPaddingTop() == 0 && getPaddingRight() == 0 && getPaddingBottom() == 0) setPadding(16, 8, 16, 8);
        if (getMinimumHeight() == 0) setMinimumHeight(48);
        if (getMinimumWidth() == 0) setMinimumWidth(88);
        if (getGravity() == 0) setGravity(android.view.Gravity.CENTER);
    }
    protected String indroidType() { return "Button"; }
}
