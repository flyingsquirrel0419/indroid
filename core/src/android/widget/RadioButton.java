package android.widget;
import android.content.Context;
import android.util.AttributeSet;
public class RadioButton extends CompoundButton {
    public RadioButton(Context c) { this(c, null); }
    public RadioButton(Context c, AttributeSet a) { this(c, a, 16842878); }
    public RadioButton(Context c, AttributeSet a, int s) { super(c, a, s); }
    public void toggle() { if (!isChecked()) super.toggle(); }
    protected String indroidType() { return "RadioButton"; }
    public RadioButton(Context c, AttributeSet a, int s, int r) { this(c,a,s); }
}
